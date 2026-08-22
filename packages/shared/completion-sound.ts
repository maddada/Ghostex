export const COMPLETION_SOUND_OPTIONS = [
  {
    fileName: "ping.mp3",
    label: "Ping",
    value: "ping",
  },
  {
    fileName: "pingdouble.mp3",
    label: "Ping Double",
    value: "pingdouble",
  },
  {
    fileName: "glass.mp3",
    label: "Glass",
    value: "glass",
  },
  {
    fileName: "glimmer.mp3",
    label: "Glimmer",
    value: "glimmer",
  },
  {
    fileName: "shamisen.mp3",
    label: "Shamisen",
    value: "shamisen",
  },
  {
    fileName: "shamisenreverb.mp3",
    label: "Shamisen Reverb",
    value: "shamisenreverb",
  },
  {
    fileName: "arcade.mp3",
    label: "Arcade",
    value: "arcade",
  },
  {
    fileName: "arcadeboost.mp3",
    label: "Arcade Boost",
    value: "arcadeboost",
  },
  {
    fileName: "confirmation-001.mp3",
    label: "Confirmation 001",
    value: "confirmation-001",
  },
  {
    fileName: "confirmation-002.mp3",
    label: "Confirmation 002",
    value: "confirmation-002",
  },
  {
    fileName: "confirmation-003.mp3",
    label: "Confirmation 003",
    value: "confirmation-003",
  },
  {
    fileName: "confirmation-004.mp3",
    label: "Confirmation 004",
    value: "confirmation-004",
  },
  {
    fileName: "notification-pop.mp3",
    label: "Notification Pop",
    value: "notification-pop",
  },
  {
    fileName: "success-chime.mp3",
    label: "Success Chime",
    value: "success-chime",
  },
  {
    fileName: "high-up.mp3",
    label: "High Up",
    value: "high-up",
  },
  {
    fileName: "high-down.mp3",
    label: "High Down",
    value: "high-down",
  },
  {
    fileName: "low-three-tone.mp3",
    label: "Low Three Tone",
    value: "low-three-tone",
  },
  {
    fileName: "tone-1.mp3",
    label: "Tone 1",
    value: "tone-1",
  },
  {
    fileName: "three-tone-1.mp3",
    label: "Three Tone 1",
    value: "three-tone-1",
  },
  {
    fileName: "three-tone-2.mp3",
    label: "Three Tone 2",
    value: "three-tone-2",
  },
  {
    fileName: "two-tone-1.mp3",
    label: "Two Tone 1",
    value: "two-tone-1",
  },
  {
    fileName: "two-tone-2.mp3",
    label: "Two Tone 2",
    value: "two-tone-2",
  },
  {
    fileName: "power-up-5.mp3",
    label: "Power Up 5",
    value: "power-up-5",
  },
  {
    fileName: "power-up-6.mp3",
    label: "Power Up 6",
    value: "power-up-6",
  },
  {
    fileName: "power-up-8.mp3",
    label: "Power Up 8",
    value: "power-up-8",
  },
  {
    fileName: "coin-collect.mp3",
    label: "Coin Collect",
    value: "coin-collect",
  },
  {
    fileName: "phaser-up-5.mp3",
    label: "Phaser Up 5",
    value: "phaser-up-5",
  },
  {
    fileName: "zap-two-tone.mp3",
    label: "Zap Two Tone",
    value: "zap-two-tone",
  },
  {
    fileName: "voiceover-pack-male-mission-completed.mp3",
    label: "Mission Completed (Male)",
    value: "voiceover-pack-male-mission-completed",
  },
  {
    fileName: "voiceover-pack-female-mission-completed.mp3",
    label: "Mission Completed (Female)",
    value: "voiceover-pack-female-mission-completed",
  },
  {
    fileName: "voiceover-pack-male-you-win.mp3",
    label: "You Win (Male)",
    value: "voiceover-pack-male-you-win",
  },
  {
    fileName: "voiceover-pack-female-congratulations.mp3",
    label: "Congratulations (Female)",
    value: "voiceover-pack-female-congratulations",
  },
  {
    fileName: "flawless-victory.mp3",
    label: "Flawless Victory",
    value: "flawless-victory",
  },
] as const;

export type CompletionSoundSetting = (typeof COMPLETION_SOUND_OPTIONS)[number]["value"];

export const DEFAULT_COMPLETION_SOUND: CompletionSoundSetting = "arcade";

export function clampCompletionSoundSetting(value: string | undefined): CompletionSoundSetting {
  return (
    COMPLETION_SOUND_OPTIONS.find((option) => option.value === value)?.value ??
    DEFAULT_COMPLETION_SOUND
  );
}

export function getCompletionSoundLabel(value: CompletionSoundSetting): string {
  return (
    COMPLETION_SOUND_OPTIONS.find((option) => option.value === value)?.label ??
    getCompletionSoundLabel(DEFAULT_COMPLETION_SOUND)
  );
}

export function getCompletionSoundFileName(value: CompletionSoundSetting): string {
  return (
    COMPLETION_SOUND_OPTIONS.find((option) => option.value === value)?.fileName ??
    getCompletionSoundFileName(DEFAULT_COMPLETION_SOUND)
  );
}
