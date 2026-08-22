import type { CompletionSoundSetting } from "../shared/completion-sound";

type CompletionSoundLogger = (event: string, details?: unknown) => void;

declare global {
  interface Window {
    __ghostex_SOUND_URLS__?: Partial<Record<CompletionSoundSetting, string>>;
    webkitAudioContext?: typeof AudioContext;
  }
}

const audioBySound = new Map<CompletionSoundSetting, HTMLAudioElement>();
const decodedBufferPromiseBySound = new Map<
  CompletionSoundSetting,
  Promise<AudioBuffer | undefined>
>();
let audioContext: AudioContext | undefined;

export async function prepareCompletionSoundPlayback(log?: CompletionSoundLogger): Promise<void> {
  const context = getAudioContext();
  if (!context) {
    log?.("completionSound.audioContextUnsupported");
    return;
  }

  if (context.state === "running") {
    return;
  }

  try {
    await context.resume();
    const source = context.createBufferSource();
    source.buffer = context.createBuffer(1, 1, context.sampleRate);
    source.connect(context.destination);
    source.start();
    log?.("completionSound.audioContextUnlocked", {
      state: context.state,
    });
  } catch (error) {
    log?.("completionSound.audioContextUnlockFailed", {
      error: error instanceof Error ? error.message : String(error),
      state: context.state,
    });
  }
}

export async function playCompletionSound(
  sound: CompletionSoundSetting,
  log?: CompletionSoundLogger,
): Promise<void> {
  const context = getAudioContext();
  if (context?.state === "running") {
    const buffer = await getDecodedBuffer(sound, context, log);
    if (!buffer) {
      return;
    }

    const source = context.createBufferSource();
    source.buffer = buffer;
    source.connect(context.destination);
    source.start();
    log?.("completionSound.played", {
      mode: "audioContext",
      sound,
    });
    return;
  }

  if (context) {
    log?.("completionSound.audioContextLockedAtPlay", {
      sound,
      state: context.state,
    });
  }

  const audio = getAudio(sound);
  if (!audio) {
    log?.("completionSound.missingAudio", {
      hasSoundUrl: Boolean(window.__ghostex_SOUND_URLS__?.[sound]),
      sound,
    });
    return;
  }

  audio.pause();
  audio.currentTime = 0;

  try {
    await audio.play();
    log?.("completionSound.played", {
      currentTime: audio.currentTime,
      mode: "htmlAudio",
      sound,
      src: audio.currentSrc || audio.src,
    });
  } catch (error) {
    log?.("completionSound.playFailed", {
      error: error instanceof Error ? error.message : String(error),
      sound,
      src: audio.currentSrc || audio.src,
    });
  }
}

function getAudioContext(): AudioContext | undefined {
  const AudioContextConstructor = window.AudioContext ?? window.webkitAudioContext;
  if (!AudioContextConstructor) {
    return undefined;
  }

  audioContext ??= new AudioContextConstructor();
  return audioContext;
}

function getAudio(sound: CompletionSoundSetting): HTMLAudioElement | undefined {
  const existingAudio = audioBySound.get(sound);
  if (existingAudio) {
    return existingAudio;
  }

  const url = window.__ghostex_SOUND_URLS__?.[sound];
  if (!url) {
    return undefined;
  }

  const audio = new Audio(url);
  audio.preload = "auto";
  audioBySound.set(sound, audio);
  return audio;
}

async function getDecodedBuffer(
  sound: CompletionSoundSetting,
  context: AudioContext,
  log?: CompletionSoundLogger,
): Promise<AudioBuffer | undefined> {
  const existingPromise = decodedBufferPromiseBySound.get(sound);
  if (existingPromise) {
    return existingPromise;
  }

  const url = window.__ghostex_SOUND_URLS__?.[sound];
  if (!url) {
    log?.("completionSound.missingAudio", {
      hasSoundUrl: false,
      sound,
    });
    return undefined;
  }

  const resolvedDecodePromise = url.startsWith("data:")
    ? context.decodeAudioData(dataUrlToArrayBuffer(url)).catch((error) => {
        decodedBufferPromiseBySound.delete(sound);
        log?.("completionSound.decodeFailed", {
          error: error instanceof Error ? error.message : String(error),
          sound,
          url: "data:",
        });
        return undefined;
      })
    : fetch(url)
        .then(async (response) => {
          if (!response.ok) {
            throw new Error(`HTTP ${response.status}`);
          }

          const arrayBuffer = await response.arrayBuffer();
          return await context.decodeAudioData(arrayBuffer.slice(0));
        })
        .catch((error) => {
          decodedBufferPromiseBySound.delete(sound);
          log?.("completionSound.decodeFailed", {
            error: error instanceof Error ? error.message : String(error),
            sound,
            url,
          });
          return undefined;
        });
  decodedBufferPromiseBySound.set(sound, resolvedDecodePromise);
  return resolvedDecodePromise;
}

function dataUrlToArrayBuffer(dataUrl: string): ArrayBuffer {
  const commaIndex = dataUrl.indexOf(",");
  if (commaIndex < 0) {
    throw new Error("Invalid data URL");
  }

  const metadata = dataUrl.slice(0, commaIndex);
  const payload = dataUrl.slice(commaIndex + 1);
  if (!/;base64$/i.test(metadata)) {
    const bytes = new TextEncoder().encode(decodeURIComponent(payload));
    return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  }

  const binary = atob(payload);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }

  return bytes.buffer;
}
