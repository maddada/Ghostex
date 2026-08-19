/*
 * Compact devtools-style form primitives shared by every control-panel section.
 * Plain CSS only (see controls.css) — the sandbox is outside the Tailwind scan.
 */
import type { ReactNode } from "react";
import { usePersistedState } from "./controls-storage";

export function Section({
  id,
  title,
  badge,
  defaultOpen = true,
  children,
}: {
  id: string;
  title: string;
  badge?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = usePersistedState(`section.${id}`, defaultOpen);
  return (
    <section className={open ? "cp-section is-open" : "cp-section"}>
      <button className="cp-section-head" onClick={() => setOpen(!open)} type="button">
        <span className="cp-caret">{open ? "▾" : "▸"}</span>
        <span className="cp-section-title">{title}</span>
        {badge === undefined ? null : <span className="cp-section-badge">{badge}</span>}
      </button>
      {open ? <div className="cp-section-body">{children}</div> : null}
    </section>
  );
}

export function SubGroup({
  id,
  title,
  defaultOpen = false,
  children,
}: {
  id: string;
  title: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = usePersistedState(`subgroup.${id}`, defaultOpen);
  return (
    <div className={open ? "cp-subgroup is-open" : "cp-subgroup"}>
      <button className="cp-subgroup-head" onClick={() => setOpen(!open)} type="button">
        <span className="cp-caret">{open ? "▾" : "▸"}</span>
        {title}
      </button>
      {open ? <div className="cp-subgroup-body">{children}</div> : null}
    </div>
  );
}

export function Row({
  label,
  hint,
  changed,
  children,
}: {
  label: ReactNode;
  hint?: ReactNode;
  changed?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={changed ? "cp-row is-changed" : "cp-row"}>
      <div className="cp-row-label">
        <span>{label}</span>
        {hint === undefined ? null : <span className="cp-row-hint">{hint}</span>}
      </div>
      <div className="cp-row-control">{children}</div>
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  title,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label?: string;
  title?: string;
}) {
  return (
    <label className="cp-toggle" title={title}>
      <input checked={checked} onChange={(e) => onChange(e.target.checked)} type="checkbox" />
      {label === undefined ? null : <span>{label}</span>}
    </label>
  );
}

export function SelectField<T extends string>({
  value,
  options,
  onChange,
  title,
}: {
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (next: T) => void;
  title?: string;
}) {
  return (
    <select
      className="cp-select"
      onChange={(e) => onChange(e.target.value as T)}
      title={title}
      value={value}
    >
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

export function Stepper({
  value,
  min = 0,
  max = 999,
  onChange,
}: {
  value: number;
  min?: number;
  max?: number;
  onChange: (next: number) => void;
}) {
  const clamp = (next: number) => Math.min(max, Math.max(min, next));
  return (
    <div className="cp-stepper">
      <button onClick={() => onChange(clamp(value - 1))} type="button">
        −
      </button>
      <input
        onChange={(e) => {
          const parsed = Number.parseInt(e.target.value, 10);
          onChange(Number.isFinite(parsed) ? clamp(parsed) : min);
        }}
        type="number"
        value={value}
      />
      <button onClick={() => onChange(clamp(value + 1))} type="button">
        +
      </button>
    </div>
  );
}

export function MsSlider({
  value,
  onChange,
  max = 5000,
  step = 50,
}: {
  value: number;
  onChange: (next: number) => void;
  max?: number;
  step?: number;
}) {
  return (
    <div className="cp-slider">
      <input
        max={max}
        min={0}
        onChange={(e) => onChange(Number(e.target.value))}
        step={step}
        type="range"
        value={value}
      />
      <span className="cp-slider-value">{value}ms</span>
    </div>
  );
}

export function Btn({
  onClick,
  children,
  disabled,
  title,
  tone = "default",
  wide,
}: {
  onClick: () => void;
  children: ReactNode;
  disabled?: boolean;
  title?: string;
  tone?: "default" | "primary" | "danger" | "ghost";
  wide?: boolean;
}) {
  return (
    <button
      className={`cp-btn cp-btn--${tone}${wide === true ? " cp-btn--wide" : ""}`}
      disabled={disabled}
      onClick={onClick}
      title={title}
      type="button"
    >
      {children}
    </button>
  );
}

export function Note({ tone = "info", children }: { tone?: "info" | "warning"; children: ReactNode }) {
  return <p className={`cp-note cp-note--${tone}`}>{children}</p>;
}
