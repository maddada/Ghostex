import { useEffect, useRef, type CSSProperties } from "react";
import { cn } from "@/packages/components/utils";
import type { PetId } from "../shared/pets";
import booSpritesheetUrl from "./assets/pets/boo-spritesheet-codexpethub-8a8161fb.webp";
import bsodSpritesheetUrl from "./assets/pets/bsod-spritesheet-v4-BRrRVy1T.webp";
import codexSpritesheetUrl from "./assets/pets/codex-spritesheet-v4-Bl6P89d_.webp";
import deweySpritesheetUrl from "./assets/pets/dewey-spritesheet-v4-gAYk_M9g.webp";
import fireballSpritesheetUrl from "./assets/pets/fireball-spritesheet-v4-BtU8R9Qp.webp";
import nullSignalSpritesheetUrl from "./assets/pets/null-signal-spritesheet-v4-CCoTR-8t.webp";
import rockySpritesheetUrl from "./assets/pets/rocky-spritesheet-v4-3RlTi26B.webp";
import seedySpritesheetUrl from "./assets/pets/seedy-spritesheet-v4-CdlE_fn9.webp";
import stackySpritesheetUrl from "./assets/pets/stacky-spritesheet-v4-CaUJd4fY.webp";

export type PetAnimationState =
  | "failed"
  | "idle"
  | "jumping"
  | "review"
  | "running"
  | "running-left"
  | "running-right"
  | "waving"
  | "waiting";

type PetAnimationFrame = {
  columnIndex: number;
  frameDurationMs: number;
  rowIndex: number;
};

const SPRITESHEET_COLUMNS = 8;
const SPRITESHEET_ROWS = 9;
const IDLE_SPEED_MULTIPLIER = 6;

const PET_SPRITESHEETS: Record<PetId, string> = {
  boo: booSpritesheetUrl,
  bsod: bsodSpritesheetUrl,
  codex: codexSpritesheetUrl,
  dewey: deweySpritesheetUrl,
  fireball: fireballSpritesheetUrl,
  "null-signal": nullSignalSpritesheetUrl,
  rocky: rockySpritesheetUrl,
  seedy: seedySpritesheetUrl,
  stacky: stackySpritesheetUrl,
};

const IDLE_FRAMES: PetAnimationFrame[] = [
  { rowIndex: 0, columnIndex: 0, frameDurationMs: 280 },
  { rowIndex: 0, columnIndex: 1, frameDurationMs: 110 },
  { rowIndex: 0, columnIndex: 2, frameDurationMs: 110 },
  { rowIndex: 0, columnIndex: 3, frameDurationMs: 140 },
  { rowIndex: 0, columnIndex: 4, frameDurationMs: 140 },
  { rowIndex: 0, columnIndex: 5, frameDurationMs: 320 },
];

const LONG_IDLE_FRAMES = IDLE_FRAMES.map((frame) => ({
  ...frame,
  frameDurationMs: frame.frameDurationMs * IDLE_SPEED_MULTIPLIER,
}));

const ANIMATION_FRAMES: Record<PetAnimationState, PetAnimationFrame[]> = {
  failed: createRowFrames(5, 8, 140, 240),
  idle: IDLE_FRAMES,
  jumping: createRowFrames(4, 5, 140, 280),
  review: createRowFrames(8, 6, 150, 280),
  running: createRowFrames(7, 6, 120, 220),
  "running-left": createRowFrames(2, 8, 120, 220),
  "running-right": createRowFrames(1, 8, 120, 220),
  waving: createRowFrames(3, 4, 140, 280),
  waiting: createRowFrames(6, 6, 150, 260),
};

export function PetAvatar({
  className,
  petId,
  reducedMotion = false,
  state = "idle",
}: {
  className?: string;
  petId: PetId;
  reducedMotion?: boolean;
  state?: PetAnimationState;
}) {
  const avatarRef = useRef<HTMLDivElement>(null);
  usePetAnimation({ avatarRef, reducedMotion, state });

  return (
    <div
      aria-hidden="true"
      className={cn("pet-avatar-root", className)}
      data-pet-id={petId}
      data-pet-state={state}
      ref={avatarRef}
      style={{ "--pet-spritesheet-url": `url(${PET_SPRITESHEETS[petId]})` } as CSSProperties}
    />
  );
}

function usePetAnimation({
  avatarRef,
  reducedMotion,
  state,
}: {
  avatarRef: React.RefObject<HTMLDivElement | null>;
  reducedMotion: boolean;
  state: PetAnimationState;
}) {
  useEffect(() => {
    const element = avatarRef.current;
    if (!element) {
      return;
    }

    const animation = resolveAnimation(state, reducedMotion);
    let frameIndex = 0;
    let timeoutId: number | undefined;

    element.style.backgroundPosition = getFramePosition(animation.frames[frameIndex]!);
    if (animation.frames.length === 1) {
      return;
    }

    const scheduleNextFrame = () => {
      timeoutId = window.setTimeout(() => {
        const nextFrameIndex = frameIndex + 1;
        if (nextFrameIndex >= animation.frames.length) {
          if (animation.loopStartIndex === undefined) {
            timeoutId = undefined;
            return;
          }
          frameIndex = animation.loopStartIndex;
        } else {
          frameIndex = nextFrameIndex;
        }
        element.style.backgroundPosition = getFramePosition(animation.frames[frameIndex]!);
        scheduleNextFrame();
      }, animation.frames[frameIndex]!.frameDurationMs);
    };

    scheduleNextFrame();
    return () => {
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [avatarRef, reducedMotion, state]);
}

function resolveAnimation(state: PetAnimationState, reducedMotion: boolean) {
  const frames = ANIMATION_FRAMES[state];
  if (reducedMotion) {
    return { frames: [frames[0]!], loopStartIndex: undefined };
  }
  if (state === "idle") {
    return { frames: LONG_IDLE_FRAMES, loopStartIndex: 0 };
  }

  const activeFrames = [...frames, ...frames, ...frames];
  return { frames: [...activeFrames, ...LONG_IDLE_FRAMES], loopStartIndex: activeFrames.length };
}

function createRowFrames(
  rowIndex: number,
  length: number,
  frameDurationMs: number,
  finalFrameDurationMs: number,
): PetAnimationFrame[] {
  return Array.from({ length }, (_, columnIndex) => ({
    columnIndex,
    frameDurationMs: columnIndex === length - 1 ? finalFrameDurationMs : frameDurationMs,
    rowIndex,
  }));
}

function getFramePosition(frame: PetAnimationFrame): string {
  return `${(frame.columnIndex / (SPRITESHEET_COLUMNS - 1)) * 100}% ${
    (frame.rowIndex / (SPRITESHEET_ROWS - 1)) * 100
  }%`;
}
