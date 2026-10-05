import { useRef } from 'react';

// A discrete slider that snaps to a fixed set of stops. Each stop is a tick on
// the track; the active one is a knob and its label sits underneath.
export type SliderStop = { value: string; label: string };

export function Slider({ value, stops, onChange, ariaLabel }: {
  value: string;
  stops: SliderStop[];
  onChange: (value: string) => void;
  ariaLabel?: string;
}) {
  const track = useRef<HTMLDivElement>(null);
  const last = Math.max(0, stops.length - 1);
  const found = stops.findIndex((s) => s.value === value);
  const index = found < 0 ? 0 : found;
  const pct = last === 0 ? 0 : (index / last) * 100;

  const pick = (clientX: number) => {
    const el = track.current;
    if (!el || last === 0) return;
    const rect = el.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    const next = stops[Math.round(ratio * last)];
    if (next && next.value !== value) onChange(next.value);
  };

  const step = (dir: number) => {
    const next = stops[Math.min(last, Math.max(0, index + dir))];
    if (next && next.value !== value) onChange(next.value);
  };

  const active = stops[index];

  return (
    <div className="px-2.5 pt-1.5 pb-2">
      <div
        ref={track}
        role="slider"
        tabIndex={0}
        aria-label={ariaLabel}
        aria-valuemin={0}
        aria-valuemax={last}
        aria-valuenow={index}
        aria-valuetext={active?.label}
        className="focus-ring relative h-7 cursor-pointer touch-none select-none rounded-lg outline-offset-2"
        onPointerDown={(e) => { e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); pick(e.clientX); }}
        onPointerMove={(e) => { if (e.buttons === 1) pick(e.clientX); }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') { e.preventDefault(); step(-1); }
          if (e.key === 'ArrowRight' || e.key === 'ArrowUp') { e.preventDefault(); step(1); }
        }}
      >
        <div className="absolute top-1/2 left-0 h-1 w-full -translate-y-1/2 rounded-full bg-border" />
        <div className="absolute top-1/2 left-0 h-1 -translate-y-1/2 rounded-full bg-foreground/60 transition-[width] duration-150" style={{ width: `${pct}%` }} />

        {stops.map((s, i) => {
          const left = last === 0 ? 0 : (i / last) * 100;
          const on = i === index;
          return (
            <span key={s.value} className="absolute top-1/2 -translate-x-1/2 -translate-y-1/2" style={{ left: `${left}%` }}>
              <span
                className={`block rounded-full transition-all duration-150 ${
                  on ? 'h-3 w-3 border-2 border-foreground bg-background' : 'h-1.5 w-1.5 bg-muted/50'
                }`}
              />
            </span>
          );
        })}
      </div>
      <div className="mt-1 text-center font-mono text-[10px] text-muted">{active?.label}</div>
    </div>
  );
}
