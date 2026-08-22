import { describe, expect, test } from "vite-plus/test";
import {
  COMPLETION_SOUND_OPTIONS,
  DEFAULT_COMPLETION_SOUND,
  clampCompletionSoundSetting,
  getCompletionSoundFileName,
  getCompletionSoundLabel,
} from "./completion-sound";

describe("completion sound settings", () => {
  test("should keep supported sound ids", () => {
    expect(clampCompletionSoundSetting("glass")).toBe("glass");
    expect(clampCompletionSoundSetting("pingdouble")).toBe("pingdouble");
  });

  test("should fall back to the default sound for unknown ids", () => {
    expect(clampCompletionSoundSetting(undefined)).toBe(DEFAULT_COMPLETION_SOUND);
    expect(clampCompletionSoundSetting("nope")).toBe(DEFAULT_COMPLETION_SOUND);
  });

  test("should expose labels and filenames for supported sounds", () => {
    expect(getCompletionSoundLabel("ping")).toBe("Ping");
    expect(getCompletionSoundFileName("ping")).toBe("ping.mp3");
    expect(getCompletionSoundLabel("success-chime")).toBe("Success Chime");
    expect(getCompletionSoundFileName("success-chime")).toBe("success-chime.mp3");
    expect(getCompletionSoundLabel("flawless-victory")).toBe("Flawless Victory");
    expect(getCompletionSoundFileName("flawless-victory")).toBe("flawless-victory.mp3");
  });

  test("should include the bundled sound variants in the picker order", () => {
    expect(COMPLETION_SOUND_OPTIONS.map((option) => option.value)).toEqual([
      "ping",
      "pingdouble",
      "glass",
      "glimmer",
      "shamisen",
      "shamisenreverb",
      "arcade",
      "arcadeboost",
      "confirmation-001",
      "confirmation-002",
      "confirmation-003",
      "confirmation-004",
      "notification-pop",
      "success-chime",
      "high-up",
      "high-down",
      "low-three-tone",
      "tone-1",
      "three-tone-1",
      "three-tone-2",
      "two-tone-1",
      "two-tone-2",
      "power-up-5",
      "power-up-6",
      "power-up-8",
      "coin-collect",
      "phaser-up-5",
      "zap-two-tone",
      "voiceover-pack-male-mission-completed",
      "voiceover-pack-female-mission-completed",
      "voiceover-pack-male-you-win",
      "voiceover-pack-female-congratulations",
      "flawless-victory",
    ]);
  });
});
