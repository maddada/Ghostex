/*
 CDXC:VoiceMode 2026-10-20 WHY:
 The voice conversation's microphone and speaker (app/voice_agent/audio.rs). gxserver's provider
 runs voice activity detection on everything the microphone hears, so the agent's own voice from
 the speakers would interrupt it: capture goes through the voice-processing I/O unit
 (`setVoiceProcessingEnabled:` on the engine's input node), whose echo canceller subtracts what this
 same engine plays. That is why playback is a player node on the same AVAudioEngine and never a
 separate audio stack. Dictation opens the engine without voice processing and without playback.
 Capture is converted to 24 kHz mono PCM16, the format gxserver relays to xAI and OpenAI unchanged.
*/

#import <AVFoundation/AVFoundation.h>
#import <AppKit/AppKit.h>
#import <AudioToolbox/AudioToolbox.h>
#import <CoreAudio/CoreAudio.h>
#import <Foundation/Foundation.h>
#include <math.h>
#include <stdatomic.h>
#include <stdint.h>
#include <string.h>

typedef void (*GhostexVoiceCaptureCallback)(void *context, const int16_t *samples,
                                            size_t count);
typedef void (*GhostexVoiceEventCallback)(void *context, int32_t event);

/* Events reported through GhostexVoiceEventCallback. */
enum {
  GhostexVoiceEventConfigurationChanged = 1,
};

/* Start results. */
enum {
  GhostexVoiceStartOk = 0,
  GhostexVoiceStartPermissionDenied = 1,
  GhostexVoiceStartNoInput = 2,
  GhostexVoiceStartFailed = 3,
};

static const double GhostexVoiceSampleRate = 24000.0;

@interface GhostexVoiceAudio : NSObject
@property(nonatomic, strong) AVAudioEngine *engine;
@property(nonatomic, strong) AVAudioPlayerNode *player;
@property(nonatomic, strong) AVAudioUnitTimePitch *timePitch;
@property(nonatomic, strong) AVAudioFormat *playbackFormat;
@property(nonatomic, strong) id configurationObserver;
@end

@implementation GhostexVoiceAudio {
@public
  _Atomic int64_t queuedFrames;
  _Atomic uint64_t generation;
}
@end

static void GhostexVoiceWriteError(char *buffer, size_t length, NSString *message) {
  if (buffer == NULL || length == 0) {
    return;
  }
  const char *utf8 = message.UTF8String ?: "";
  strncpy(buffer, utf8, length - 1);
  buffer[length - 1] = '\0';
}

/* Asks for microphone access when it was never asked, waiting for the answer. Must not run on the
   main thread: the prompt's completion is delivered on an arbitrary queue. */
static BOOL GhostexVoiceMicrophoneAllowed(void) {
  AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
  if (status == AVAuthorizationStatusAuthorized) {
    return YES;
  }
  if (status != AVAuthorizationStatusNotDetermined) {
    return NO;
  }
  __block BOOL granted = NO;
  dispatch_semaphore_t answered = dispatch_semaphore_create(0);
  [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio
                           completionHandler:^(BOOL allowed) {
                             granted = allowed;
                             dispatch_semaphore_signal(answered);
                           }];
  dispatch_semaphore_wait(answered, dispatch_time(DISPATCH_TIME_NOW, 120 * NSEC_PER_SEC));
  return granted;
}

/* 0 authorized, 1 denied or restricted, 2 never asked. */
int32_t GhostexVoiceMicrophoneStatus(void) {
  AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
  switch (status) {
  case AVAuthorizationStatusAuthorized:
    return 0;
  case AVAuthorizationStatusNotDetermined:
    return 2;
  default:
    return 1;
  }
}

/* The CoreAudio devices that have input streams. */
static NSArray<NSNumber *> *GhostexVoiceInputDeviceIDs(void) {
  AudioObjectPropertyAddress devicesAddress = {kAudioHardwarePropertyDevices,
                                               kAudioObjectPropertyScopeGlobal,
                                               kAudioObjectPropertyElementMain};
  UInt32 size = 0;
  if (AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &devicesAddress, 0, NULL, &size) !=
          noErr ||
      size == 0) {
    return @[];
  }
  UInt32 count = size / sizeof(AudioDeviceID);
  AudioDeviceID *devices = calloc(count, sizeof(AudioDeviceID));
  if (devices == NULL) {
    return @[];
  }
  NSMutableArray<NSNumber *> *inputs = [NSMutableArray array];
  if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &devicesAddress, 0, NULL, &size,
                                 devices) == noErr) {
    AudioObjectPropertyAddress streamsAddress = {kAudioDevicePropertyStreams,
                                                 kAudioObjectPropertyScopeInput,
                                                 kAudioObjectPropertyElementMain};
    for (UInt32 index = 0; index < count; index++) {
      UInt32 streamsSize = 0;
      if (AudioObjectGetPropertyDataSize(devices[index], &streamsAddress, 0, NULL, &streamsSize) ==
              noErr &&
          streamsSize > 0) {
        [inputs addObject:@(devices[index])];
      }
    }
  }
  free(devices);
  return inputs;
}

static NSString *GhostexVoiceDeviceName(AudioDeviceID device) {
  AudioObjectPropertyAddress nameAddress = {kAudioObjectPropertyName,
                                            kAudioObjectPropertyScopeGlobal,
                                            kAudioObjectPropertyElementMain};
  CFStringRef name = NULL;
  UInt32 size = sizeof(name);
  if (AudioObjectGetPropertyData(device, &nameAddress, 0, NULL, &size, &name) != noErr ||
      name == NULL) {
    return nil;
  }
  return (__bridge_transfer NSString *)name;
}

static AudioDeviceID GhostexVoiceDefaultInputDevice(void) {
  AudioObjectPropertyAddress defaultAddress = {kAudioHardwarePropertyDefaultInputDevice,
                                               kAudioObjectPropertyScopeGlobal,
                                               kAudioObjectPropertyElementMain};
  AudioDeviceID device = kAudioObjectUnknown;
  UInt32 size = sizeof(device);
  if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &defaultAddress, 0, NULL, &size,
                                 &device) != noErr) {
    return kAudioObjectUnknown;
  }
  return device;
}

/* The input devices as lines of `D\t<name>` (the default input) or `-\t<name>`, for Settings >
   Voice's microphone picker. Returns the bytes written, 0 when there is nothing to list. */
size_t GhostexVoiceInputDevices(char *buffer, size_t length) {
  @autoreleasepool {
    if (buffer == NULL || length == 0) {
      return 0;
    }
    buffer[0] = 0;
    AudioDeviceID defaultDevice = GhostexVoiceDefaultInputDevice();
    NSMutableString *lines = [NSMutableString string];
    for (NSNumber *device in GhostexVoiceInputDeviceIDs()) {
      NSString *name = GhostexVoiceDeviceName(device.unsignedIntValue);
      if (name.length == 0) {
        continue;
      }
      NSString *clean = [[name stringByReplacingOccurrencesOfString:@"\n" withString:@" "]
          stringByReplacingOccurrencesOfString:@"\t"
                                    withString:@" "];
      [lines appendFormat:@"%@\t%@\n", device.unsignedIntValue == defaultDevice ? @"D" : @"-",
                          clean];
    }
    const char *utf8 = lines.UTF8String;
    size_t needed = utf8 == NULL ? 0 : strlen(utf8);
    if (needed == 0 || needed + 1 > length) {
      return 0;
    }
    memcpy(buffer, utf8, needed + 1);
    return needed;
  }
}

/* The input device named `name`, or kAudioObjectUnknown. */
static AudioDeviceID GhostexVoiceInputDeviceNamed(const char *name) {
  if (name == NULL || name[0] == 0) {
    return kAudioObjectUnknown;
  }
  NSString *wanted = [NSString stringWithUTF8String:name];
  for (NSNumber *device in GhostexVoiceInputDeviceIDs()) {
    if ([GhostexVoiceDeviceName(device.unsignedIntValue) isEqualToString:wanted]) {
      return device.unsignedIntValue;
    }
  }
  return kAudioObjectUnknown;
}

/* Starts the engine. `capture` receives 24 kHz mono PCM16 on the audio thread; `event` reports a
   device change, after which the caller stops and starts again. `inputName` (NULL for the
   default) names the microphone Settings > Voice picked; a device that is not connected falls back
   to the default. Returns the handle in `out`. */
int32_t GhostexVoiceAudioStart(int32_t voiceProcessing, int32_t playback, const char *inputName,
                               GhostexVoiceCaptureCallback capture,
                               GhostexVoiceEventCallback event, void *context, void **out,
                               char *errorBuffer, size_t errorLength) {
  @autoreleasepool {
    *out = NULL;
    if (!GhostexVoiceMicrophoneAllowed()) {
      GhostexVoiceWriteError(errorBuffer, errorLength, @"Microphone access was denied.");
      return GhostexVoiceStartPermissionDenied;
    }
    GhostexVoiceAudio *audio = [GhostexVoiceAudio new];
    atomic_store(&audio->queuedFrames, 0);
    atomic_store(&audio->generation, 0);
    AVAudioEngine *engine = [AVAudioEngine new];
    audio.engine = engine;
    AVAudioInputNode *input = engine.inputNode;
    NSError *error = nil;
    /* CDXC:VoiceMode 2026-11-05 WHY: Dictate's microphone picker (Q1264). AVAudioEngine always
       opens the system default input; pointing its input unit at another device before the
       format is read makes the tap record from it. A conversation (voice processing) keeps the
       default, whose echo canceller is paired with the default output. */
    AudioDeviceID chosenInput = voiceProcessing ? kAudioObjectUnknown
                                                : GhostexVoiceInputDeviceNamed(inputName);
    if (chosenInput != kAudioObjectUnknown && input.audioUnit != NULL) {
      (void)AudioUnitSetProperty(input.audioUnit, kAudioOutputUnitProperty_CurrentDevice,
                                 kAudioUnitScope_Global, 0, &chosenInput, sizeof(chosenInput));
    }
    if (voiceProcessing) {
      /* CDXC:VoiceMode 2026-10-20 WHY: the output side has to be in the graph before voice
         processing is turned on. Turning it on first and connecting the player afterwards fails to
         start with -10875 (kAudioUnitErr_FailedInitialization) on Bluetooth earbuds (16 kHz
         headset input, 44.1 kHz output), and without an output the processed input delivers no
         frames at all. */
      (void)engine.outputNode;
      (void)engine.mainMixerNode;
      if (![input setVoiceProcessingEnabled:YES error:&error]) {
        GhostexVoiceWriteError(
            errorBuffer, errorLength,
            [NSString stringWithFormat:@"Echo cancellation could not start: %@",
                                       error.localizedDescription ?: @"unknown error"]);
        return GhostexVoiceStartFailed;
      }
      if (@available(macOS 14.0, *)) {
        /* Keep other apps' audio at its volume while a conversation is open. */
        AVAudioVoiceProcessingOtherAudioDuckingConfiguration ducking;
        ducking.enableAdvancedDucking = NO;
        ducking.duckingLevel = AVAudioVoiceProcessingOtherAudioDuckingLevelMin;
        input.voiceProcessingOtherAudioDuckingConfiguration = ducking;
      }
    }
    AVAudioFormat *inputFormat = [input outputFormatForBus:0];
    if (inputFormat.sampleRate <= 0 || inputFormat.channelCount == 0) {
      GhostexVoiceWriteError(errorBuffer, errorLength, @"No microphone was found.");
      return GhostexVoiceStartNoInput;
    }
    AVAudioFormat *mono = [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatFloat32
                                                           sampleRate:inputFormat.sampleRate
                                                             channels:1
                                                          interleaved:NO];
    AVAudioFormat *target = [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatInt16
                                                             sampleRate:GhostexVoiceSampleRate
                                                               channels:1
                                                            interleaved:YES];
    AVAudioConverter *converter = [[AVAudioConverter alloc] initFromFormat:mono toFormat:target];
    if (converter == nil) {
      GhostexVoiceWriteError(errorBuffer, errorLength,
                             @"The microphone's format could not be converted.");
      return GhostexVoiceStartFailed;
    }
    double ratio = GhostexVoiceSampleRate / inputFormat.sampleRate;
    [input installTapOnBus:0
                bufferSize:(AVAudioFrameCount)(inputFormat.sampleRate / 50.0)
                    format:inputFormat
                     block:^(AVAudioPCMBuffer *buffer, AVAudioTime *when) {
                       (void)when;
                       AVAudioFrameCount frames = buffer.frameLength;
                       if (frames == 0) {
                         return;
                       }
                       /* The voice-processing unit can report several input channels; the
                          first is the processed microphone. */
                       AVAudioPCMBuffer *monoBuffer =
                           [[AVAudioPCMBuffer alloc] initWithPCMFormat:mono frameCapacity:frames];
                       monoBuffer.frameLength = frames;
                       float *destination = monoBuffer.floatChannelData[0];
                       if (buffer.floatChannelData != NULL) {
                         memcpy(destination, buffer.floatChannelData[0], frames * sizeof(float));
                       } else if (buffer.int16ChannelData != NULL) {
                         const int16_t *source = buffer.int16ChannelData[0];
                         NSUInteger stride = buffer.stride;
                         for (AVAudioFrameCount i = 0; i < frames; i++) {
                           destination[i] = source[i * stride] / 32768.0f;
                         }
                       } else {
                         return;
                       }
                       AVAudioFrameCount capacity = (AVAudioFrameCount)(frames * ratio) + 64;
                       AVAudioPCMBuffer *converted =
                           [[AVAudioPCMBuffer alloc] initWithPCMFormat:target
                                                         frameCapacity:capacity];
                       __block BOOL supplied = NO;
                       NSError *conversionError = nil;
                       [converter
                           convertToBuffer:converted
                                     error:&conversionError
                        withInputFromBlock:^AVAudioBuffer *(AVAudioPacketCount count,
                                                            AVAudioConverterInputStatus *status) {
                          (void)count;
                          if (supplied) {
                            *status = AVAudioConverterInputStatus_NoDataNow;
                            return nil;
                          }
                          supplied = YES;
                          *status = AVAudioConverterInputStatus_HaveData;
                          return monoBuffer;
                        }];
                       if (converted.frameLength > 0 && converted.int16ChannelData != NULL) {
                         capture(context, converted.int16ChannelData[0], converted.frameLength);
                       }
                     }];
    if (playback) {
      AVAudioPlayerNode *player = [AVAudioPlayerNode new];
      AVAudioFormat *playbackFormat =
          [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatFloat32
                                           sampleRate:GhostexVoiceSampleRate
                                             channels:1
                                          interleaved:NO];
      /* CDXC:VoiceMode 2026-10-20 WHY: the speed buttons go up to 2x, past what either provider
         generates (xAI 0.7-1.5, OpenAI 0.25-1.5 and only between turns), so the agent's audio is
         sped up here with its pitch kept, which also changes it mid-reply. The player still counts
         the provider's samples, so the played position reported at an interruption stays in the
         provider's audio time. Bypassed at 1x. */
      AVAudioUnitTimePitch *timePitch = [AVAudioUnitTimePitch new];
      timePitch.rate = 1.0f;
      timePitch.bypass = YES;
      [engine attachNode:player];
      [engine attachNode:timePitch];
      [engine connect:player to:timePitch format:playbackFormat];
      [engine connect:timePitch to:engine.mainMixerNode format:playbackFormat];
      audio.player = player;
      audio.timePitch = timePitch;
      audio.playbackFormat = playbackFormat;
    }
    [engine prepare];
    if (![engine startAndReturnError:&error]) {
      [input removeTapOnBus:0];
      GhostexVoiceWriteError(
          errorBuffer, errorLength,
          [NSString stringWithFormat:@"The microphone could not start: %@",
                                     error.localizedDescription ?: @"unknown error"]);
      return GhostexVoiceStartFailed;
    }
    if (audio.player != nil) {
      [audio.player play];
    }
    __weak GhostexVoiceAudio *weakAudio = audio;
    audio.configurationObserver = [[NSNotificationCenter defaultCenter]
        addObserverForName:AVAudioEngineConfigurationChangeNotification
                    object:engine
                     queue:nil
                usingBlock:^(NSNotification *notification) {
                  (void)notification;
                  if (weakAudio != nil) {
                    event(context, GhostexVoiceEventConfigurationChanged);
                  }
                }];
    *out = (__bridge_retained void *)audio;
    return GhostexVoiceStartOk;
  }
}

/* Queues agent audio (PCM16 24 kHz mono, `count` samples) after what is already playing. */
void GhostexVoiceAudioEnqueue(void *handle, const int16_t *samples, size_t count) {
  if (handle == NULL || samples == NULL || count == 0) {
    return;
  }
  @autoreleasepool {
    GhostexVoiceAudio *audio = (__bridge GhostexVoiceAudio *)handle;
    @synchronized(audio) {
      AVAudioPlayerNode *player = audio.player;
      if (player == nil) {
        return;
      }
      AVAudioPCMBuffer *buffer =
          [[AVAudioPCMBuffer alloc] initWithPCMFormat:audio.playbackFormat
                                        frameCapacity:(AVAudioFrameCount)count];
      buffer.frameLength = (AVAudioFrameCount)count;
      float *destination = buffer.floatChannelData[0];
      for (size_t i = 0; i < count; i++) {
        destination[i] = samples[i] / 32768.0f;
      }
      uint64_t generation = atomic_load(&audio->generation);
      int64_t frames = (int64_t)count;
      atomic_fetch_add(&audio->queuedFrames, frames);
      [player scheduleBuffer:buffer
          completionCallbackType:AVAudioPlayerNodeCompletionDataPlayedBack
               completionHandler:^(AVAudioPlayerNodeCompletionCallbackType type) {
                 (void)type;
                 if (atomic_load(&audio->generation) == generation) {
                   atomic_fetch_sub(&audio->queuedFrames, frames);
                 }
               }];
      if (!player.isPlaying && audio.engine.isRunning) {
        [player play];
      }
    }
  }
}

/* Plays the agent's audio `rate` times as fast (1 is as sent), keeping its pitch. */
void GhostexVoiceAudioSetRate(void *handle, float rate) {
  if (handle == NULL || !(rate > 0.0f)) {
    return;
  }
  @autoreleasepool {
    GhostexVoiceAudio *audio = (__bridge GhostexVoiceAudio *)handle;
    @synchronized(audio) {
      AVAudioUnitTimePitch *timePitch = audio.timePitch;
      if (timePitch == nil) {
        return;
      }
      timePitch.rate = rate;
      timePitch.bypass = fabsf(rate - 1.0f) < 0.001f;
    }
  }
}

/* Stops playback at once and drops everything queued. Returns how many samples were dropped. */
int64_t GhostexVoiceAudioFlush(void *handle) {
  if (handle == NULL) {
    return 0;
  }
  @autoreleasepool {
    GhostexVoiceAudio *audio = (__bridge GhostexVoiceAudio *)handle;
    @synchronized(audio) {
      int64_t dropped = atomic_load(&audio->queuedFrames);
      atomic_fetch_add(&audio->generation, 1);
      atomic_store(&audio->queuedFrames, 0);
      if (audio.player != nil) {
        [audio.player stop];
        if (audio.engine.isRunning) {
          [audio.player play];
        }
      }
      return dropped < 0 ? 0 : dropped;
    }
  }
}

/* Stops the engine and releases the handle. No callback runs after it returns. */
void GhostexVoiceAudioStop(void *handle) {
  if (handle == NULL) {
    return;
  }
  @autoreleasepool {
    GhostexVoiceAudio *audio = (__bridge_transfer GhostexVoiceAudio *)handle;
    @synchronized(audio) {
      if (audio.configurationObserver != nil) {
        [[NSNotificationCenter defaultCenter] removeObserver:audio.configurationObserver];
        audio.configurationObserver = nil;
      }
      atomic_fetch_add(&audio->generation, 1);
      [audio.engine.inputNode removeTapOnBus:0];
      if (audio.player != nil) {
        [audio.player stop];
      }
      [audio.engine stop];
      audio.player = nil;
      audio.timePitch = nil;
      audio.engine = nil;
    }
  }
}

// Whether the focused text field of the key window holds uncommitted IME text (a composition in
// progress), whichever field it is: the chat box, a terminal or any other GPUI field answer through
// GPUI's view, a CEF page through Chromium's. The voice agent's follow leaves the user where they
// are meanwhile (app/voice_agent/typing.rs). Main thread only.
int32_t GhostexVoiceKeyWindowIsComposingText(void) {
  NSResponder *responder = NSApp.keyWindow.firstResponder;
  if (responder == nil || ![responder respondsToSelector:@selector(hasMarkedText)]) {
    return 0;
  }
  return [(id<NSTextInputClient>)responder hasMarkedText] ? 1 : 0;
}
