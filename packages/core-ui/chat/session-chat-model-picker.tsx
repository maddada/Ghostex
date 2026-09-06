import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { Dialog } from '@base-ui/react/dialog';
import { ModelPickerEffortIcon } from './session-chat-model-picker-effort-icons';
import { ModelPickerIcon } from './session-chat-model-picker-icons';
import {
  modelPickerControlForKey,
  useModelPickerKeyFeedback,
  useModelPickerWheelNavigation,
  type PickerControl,
} from './session-chat-model-picker-input';
import './session-chat-model-picker.css';

export interface ModelPickerModel {
  value: string;
  label: string;
  version?: string;
  efforts: { value: string; label: string }[];
  defaultEffort?: string;
}
export interface ModelPickerRequest {
  requestId: string;
  provider: 'codex' | 'claude';
  models: ModelPickerModel[];
  efforts: { value: string; label: string }[];
  model: string;
  effort: string;
}
export interface ModelPickerSelection {
  model: string;
  effort: string;
}

/**
 * CDXC:SessionChat 2026-09-05 DECISION:
 * User: match the floating Electric Axis concept inside the actual chat pane, never a separate native modal window.
 * This supersedes the original native child-window hosting for this picker only.
 * Models run vertically in catalog order (Astra, Sol, Terra, Luna; Fable, Opus (1m), Opus, Sonnet, Haiku), with the selected model at the axis intersection.
 * Keep the concept's rounded tiles, individual model artwork, connecting axes, arrow cues and animated glow, using OpenAI #0069cb and Claude #e85c35.
 * Option+P opens even during a turn and pressing the opening hotkey again cancels; arrows or H/J/K/L preview model and effort, Enter saves, and Escape cancels.
 * Model navigation stops at both ends; axis words are omitted, Sol is a sun, Luna is a fine crescent, and Sonnet's old quill artwork is replaced.
 * User: hide the effort below the model on wide panes; at 700px or less, show it there and hide the horizontal effort cards.
 * Narrow panes retain clickable effort arrows beside the selected model; trackpad gestures navigate both axes.
 * Short panes clip the model rail with at least three cards visible; controls stay at the bottom, and idle cards and background stay subtly animated.
 * Unsupported efforts stay visible but disabled in the horizontal rail; Max animates gently, Ultra more energetically, with a short glow beneath the effort.
 */
export function SessionChatModelPicker({
  request,
  container,
  onSave,
  onClose,
  cancelRequested = false,
}: {
  request: ModelPickerRequest;
  container: HTMLElement;
  onSave: (selection: ModelPickerSelection) => void;
  onClose: () => void;
  cancelRequested?: boolean;
}) {
  const [selection, setSelection] = useState<ModelPickerSelection>({ model: request.model, effort: request.effort });
  const [closing, setClosing] = useState(false);
  const [committing, setCommitting] = useState(false);
  const [paneSize, setPaneSize] = useState({ width: container.clientWidth, height: container.clientHeight });
  const [controlsHeight, setControlsHeight] = useState(56);
  const { pressed, press, pulse } = useModelPickerKeyFeedback();
  const [popup, setPopup] = useState<HTMLDivElement | null>(null);
  const closingRef = useRef(false);
  const selectedTile = useRef<HTMLButtonElement>(null);
  const [controls, setControls] = useState<HTMLElement | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const modelIndex = request.models.findIndex((model) => model.value === selection.model);
  const model = request.models[modelIndex]!;
  const effortIndex = request.efforts.findIndex((effort) => effort.value === selection.effort);
  const narrow = paneSize.width <= 700;
  const viewportHeight = Math.max(1, paneSize.height - controlsHeight - 24);
  // Keep at least three substantial cards in view instead of shrinking the entire rail to fit.
  const scale = Math.max(
    0.01,
    Math.min(1, (paneSize.width - 28) / (narrow ? 240 : 1060), (viewportHeight - 24) / (3 * 142))
  );
  const visibleModels = Math.min(request.models.length, Math.max(3, Math.floor((viewportHeight - 24) / (142 * scale))));
  const firstVisible = Math.max(
    0,
    Math.min(request.models.length - visibleModels, modelIndex - Math.floor(visibleModels / 2))
  );
  const stageHeight = viewportHeight / scale;
  const railOffset = (stageHeight - visibleModels * 142) / 2 - firstVisible * 142;
  const centerY = 71 + modelIndex * 142 + railOffset;
  const effortSplit = Math.ceil(request.efforts.length / 2);

  useEffect(() => () => clearTimeout(timer.current), []);
  useLayoutEffect(() => {
    const size = () => {
      setPaneSize({ width: container.clientWidth, height: container.clientHeight });
      if (controls) setControlsHeight(controls.offsetHeight);
    };
    size();
    const observer = new ResizeObserver(size);
    observer.observe(container);
    if (controls) observer.observe(controls);
    return () => observer.disconnect();
  }, [container, controls]);
  useEffect(() => {
    selectedTile.current?.focus({ preventScroll: true });
  }, [selection.model, selection.effort]);

  const finish = (save: boolean) => {
    if (closingRef.current) return;
    closingRef.current = true;
    setCommitting(save);
    setClosing(true);
    timer.current = setTimeout(
      () => (save ? onSave(selection) : onClose()),
      window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 190
    );
  };
  useEffect(() => {
    if (cancelRequested) finish(false);
  }, [cancelRequested]);
  const chooseModel = (index: number) => {
    const next = request.models[index];
    if (!next || closingRef.current) return;
    setSelection((current) => ({
      model: next.value,
      effort: next.efforts.some((effort) => effort.value === current.effort)
        ? current.effort
        : (next.efforts.find((effort) => effort.value === next.defaultEffort)?.value ?? next.efforts[0]?.value ?? ''),
    }));
  };
  const chooseEffort = (index: number) => {
    const next = request.efforts[index];
    if (next && model.efforts.some((entry) => entry.value === next.value) && !closingRef.current)
      setSelection((current) => ({ ...current, effort: next.value }));
  };
  const moveEffort = (direction: number) => {
    for (let index = effortIndex + direction; index >= 0 && index < request.efforts.length; index += direction) {
      if (model.efforts.some((entry) => entry.value === request.efforts[index]?.value)) {
        chooseEffort(index);
        return;
      }
    }
  };
  const canMoveEffort = (direction: number) =>
    request.efforts.some(
      (entry, index) =>
        (index - effortIndex) * direction > 0 && model.efforts.some((supported) => supported.value === entry.value)
    );
  const navigate = (control: PickerControl) => {
    pulse(control);
    if (control === 'ArrowUp') chooseModel(modelIndex - 1);
    if (control === 'ArrowDown') chooseModel(modelIndex + 1);
    if (control === 'ArrowLeft') moveEffort(-1);
    if (control === 'ArrowRight') moveEffort(1);
    if (control === 'Enter') finish(true);
    if (control === 'Escape') finish(false);
  };
  useModelPickerWheelNavigation(popup, navigate);
  const effortX = (index: number) => (index < effortSplit ? index - effortSplit : index - effortSplit + 1) * 142;
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) finish(false);
      }}
    >
      <Dialog.Portal container={container}>
        <Dialog.Backdrop className='ghostex-model-picker-backdrop' data-closing={closing ? '' : undefined} />
        <Dialog.Popup
          ref={setPopup}
          className='ghostex-model-picker'
          initialFocus={selectedTile}
          data-closing={closing ? '' : undefined}
          data-saving={committing ? '' : undefined}
          data-narrow={narrow ? '' : undefined}
          data-compact-controls={paneSize.width < 560 ? '' : undefined}
          style={{ '--picker-accent': request.provider === 'codex' ? '#0069cb' : '#e85c35' } as CSSProperties}
          onPointerDown={(event) => {
            if (!(event.target as Element).closest('button')) finish(false);
          }}
          onKeyDownCapture={(event) => {
            const control = modelPickerControlForKey(event.nativeEvent);
            if (!control) return;
            event.preventDefault();
            event.stopPropagation();
            press(event.nativeEvent, control);
            navigate(control);
          }}
        >
          <Dialog.Title className='model-picker-sr-only'>Choose model and effort</Dialog.Title>
          <Dialog.Description className='model-picker-sr-only'>
            Up and down choose a model. Left and right choose effort. Enter saves. Escape cancels.
          </Dialog.Description>
          <div className='model-picker-atmosphere' aria-hidden='true'>
            <div className='model-picker-nebula' />
            <div className='model-picker-nebula model-picker-nebula-secondary' />
            {Array.from({ length: 16 }, (_, index) => (
              <i
                key={index}
                style={
                  {
                    '--particle-x': `${(index * 37 + 11) % 100}%`,
                    '--particle-y': `${(index * 23 + 7) % 100}%`,
                    '--drift-delay': `${index * -1.7}s`,
                  } as CSSProperties
                }
              />
            ))}
          </div>
          <div className='model-picker-viewport' style={{ height: viewportHeight }}>
            <div
              className='model-picker-stage'
              style={
                {
                  height: stageHeight,
                  transform: `translate(-50%, -50%) scale(${scale})`,
                  '--center-y': `${centerY}px`,
                } as CSSProperties
              }
            >
              <div
                className='model-picker-line model-picker-line-vertical'
                aria-hidden='true'
                style={{ top: 71 + railOffset, height: (request.models.length - 1) * 142 }}
              />
              {narrow && (
                <>
                  <button
                    className='model-picker-inline-effort'
                    type='button'
                    aria-label='Decrease effort'
                    disabled={closing || !canMoveEffort(-1)}
                    onClick={() => navigate('ArrowLeft')}
                  >
                    <svg viewBox='0 0 20 20'>
                      <path d='m12 4-6 6 6 6' />
                    </svg>
                  </button>
                  <button
                    className='model-picker-inline-effort model-picker-inline-effort-next'
                    type='button'
                    aria-label='Increase effort'
                    disabled={closing || !canMoveEffort(1)}
                    onClick={() => navigate('ArrowRight')}
                  >
                    <svg viewBox='0 0 20 20'>
                      <path d='m8 4 6 6-6 6' />
                    </svg>
                  </button>
                </>
              )}
              {!narrow && request.efforts.length > 0 && (
                <div className='model-picker-line model-picker-line-horizontal' aria-hidden='true' />
              )}
              <button
                className='model-picker-axis-label model-picker-axis-top'
                style={{ top: Math.max(4, 71 + railOffset - 78) }}
                type='button'
                onClick={() => chooseModel(modelIndex - 1)}
                disabled={modelIndex === 0}
                aria-label='Previous model'
              >
                <i className='model-picker-triangle model-picker-triangle-up' />
              </button>
              <button
                className='model-picker-axis-label model-picker-axis-bottom'
                style={{
                  top: Math.min(stageHeight - 10, 71 + railOffset + (request.models.length - 1) * 142 + 76),
                  bottom: 'auto',
                }}
                type='button'
                onClick={() => chooseModel(modelIndex + 1)}
                disabled={modelIndex === request.models.length - 1}
                aria-label='Next model'
              >
                <i className='model-picker-triangle model-picker-triangle-down' />
              </button>
              <button
                className='model-picker-chevron model-picker-chevron-up'
                type='button'
                onClick={() => chooseModel(modelIndex - 1)}
                disabled={modelIndex === 0}
                aria-label='Move up one model'
              >
                <svg viewBox='0 0 20 12'>
                  <path d='m3 9 7-6 7 6' />
                </svg>
              </button>
              <button
                className='model-picker-chevron model-picker-chevron-down'
                type='button'
                onClick={() => chooseModel(modelIndex + 1)}
                disabled={modelIndex === request.models.length - 1}
                aria-label='Move down one model'
              >
                <svg viewBox='0 0 20 12'>
                  <path d='m3 3 7 6 7-6' />
                </svg>
              </button>
              {request.models.map((entry, index) => {
                const offset = index - modelIndex;
                return (
                  <button
                    key={entry.value}
                    ref={offset === 0 ? selectedTile : undefined}
                    type='button'
                    className='model-picker-tile model-picker-model'
                    data-selected={offset === 0 ? '' : undefined}
                    aria-pressed={offset === 0}
                    aria-label={`Model ${entry.label}`}
                    disabled={closing}
                    style={
                      {
                        top: 71 + railOffset,
                        '--tile-x': '0px',
                        '--tile-y': `${index * 142}px`,
                        '--idle-delay': `${index * -1.3}s`,
                      } as CSSProperties
                    }
                    tabIndex={index >= firstVisible && index < firstVisible + visibleModels ? 0 : -1}
                    onClick={() => chooseModel(index)}
                  >
                    <span className='model-picker-artwork'>
                      <ModelPickerIcon model={entry.value} />
                    </span>
                    <span className='model-picker-model-name'>
                      {entry.version && <span className='model-picker-model-version'>{entry.version}</span>}
                      {entry.label}
                    </span>
                    {narrow && offset === 0 && (
                      <span className='model-picker-current-effort' key={selection.effort}>
                        {request.efforts[effortIndex]?.label ?? 'No effort setting'}
                      </span>
                    )}
                  </button>
                );
              })}
              {!narrow && request.efforts.length > 0 && (
                <>
                  <button
                    className='model-picker-axis-label model-picker-axis-left'
                    type='button'
                    onClick={() => moveEffort(-1)}
                    disabled={!canMoveEffort(-1)}
                    aria-label='Decrease effort'
                  >
                    <i className='model-picker-triangle model-picker-triangle-left' />
                  </button>
                  <button
                    className='model-picker-axis-label model-picker-axis-right'
                    type='button'
                    onClick={() => moveEffort(1)}
                    disabled={!canMoveEffort(1)}
                    aria-label='Increase effort'
                  >
                    <i className='model-picker-triangle model-picker-triangle-right' />
                  </button>
                  <div
                    hidden={effortIndex < 0}
                    className='model-picker-effort-aura'
                    aria-hidden='true'
                    style={{ '--tile-x': `${effortX(effortIndex)}px` } as CSSProperties}
                  />
                  {request.efforts.map((entry, index) => (
                    <button
                      key={entry.value}
                      type='button'
                      className='model-picker-tile model-picker-effort'
                      data-effort={entry.value}
                      data-unavailable={!model.efforts.some((effort) => effort.value === entry.value) ? '' : undefined}
                      data-selected={index === effortIndex ? '' : undefined}
                      aria-pressed={index === effortIndex}
                      aria-label={`Effort ${entry.label}`}
                      disabled={closing || !model.efforts.some((effort) => effort.value === entry.value)}
                      title={
                        !model.efforts.some((effort) => effort.value === entry.value)
                          ? `${entry.label} is unavailable for ${model.label}`
                          : undefined
                      }
                      style={
                        {
                          '--tile-x': `${effortX(index)}px`,
                          '--tile-y': '0px',
                          '--idle-delay': `${index * -1.1}s`,
                        } as CSSProperties
                      }
                      onClick={() => chooseEffort(index)}
                    >
                      <ModelPickerEffortIcon effort={entry.value} />
                      <span>{entry.label}</span>
                    </button>
                  ))}
                </>
              )}
              <span className='model-picker-sr-only' role='status' aria-live='polite'>
                {model.label}, {request.efforts[effortIndex]?.label ?? 'No effort setting'}
              </span>
            </div>
          </div>
          <footer ref={setControls} className='model-picker-help'>
            <span className='model-picker-open-hint' data-key-pressed={cancelRequested ? '' : undefined}>
              <kbd>⌥</kbd>
              <kbd>P</kbd>
              <span>Close</span>
            </span>
            <span>
              <button
                type='button'
                aria-label='Previous model'
                disabled={closing || modelIndex === 0}
                data-key-pressed={pressed.has('ArrowUp') ? '' : undefined}
                onClick={() => chooseModel(modelIndex - 1)}
              >
                <kbd>↑</kbd>
              </button>
              <button
                type='button'
                aria-label='Next model'
                disabled={closing || modelIndex === request.models.length - 1}
                data-key-pressed={pressed.has('ArrowDown') ? '' : undefined}
                onClick={() => chooseModel(modelIndex + 1)}
              >
                <kbd>↓</kbd>
              </button>
              <span>Model</span>
            </span>
            <span>
              <button
                type='button'
                aria-label='Decrease effort'
                disabled={closing || !canMoveEffort(-1)}
                data-key-pressed={pressed.has('ArrowLeft') ? '' : undefined}
                onClick={() => moveEffort(-1)}
              >
                <kbd>←</kbd>
              </button>
              <button
                type='button'
                aria-label='Increase effort'
                disabled={closing || !canMoveEffort(1)}
                data-key-pressed={pressed.has('ArrowRight') ? '' : undefined}
                onClick={() => moveEffort(1)}
              >
                <kbd>→</kbd>
              </button>
              <span>Effort</span>
            </span>
            <button
              type='button'
              data-key-pressed={pressed.has('Enter') ? '' : undefined}
              disabled={closing}
              onClick={() => finish(true)}
            >
              <kbd>↵</kbd>
              <span>Save</span>
            </button>
            <button
              type='button'
              data-key-pressed={pressed.has('Escape') ? '' : undefined}
              disabled={closing}
              onClick={() => finish(false)}
            >
              <kbd>Esc</kbd>
              <span>Cancel</span>
            </button>
          </footer>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
