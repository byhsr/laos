import { useRef } from 'react';

// One slider primitive, two shapes:
//  - `stops` — a discrete slider that snaps to a fixed set of stops; each stop
//    is a tick on the track and the active one is the knob.
//  - `percent` — a continuous 0–100 track with no ticks, for a pref that is a
//    plain percentage (frost strength) rather than a named set.
export type SliderStop = { value: string; label: string };

type StopsSlider = { value: string; stops: SliderStop[]; onChange: (value: string) => void; ariaLabel?: string; percent?: false };
type PercentSlider = { value: number; onChange: (value: number) => void; ariaLabel?: string; percent: true };
export type SliderProps = StopsSlider | PercentSlider;

export function Slider(props: SliderProps) {
  const track = useRef<HTMLDivElement>(null);

  // Both shapes reduce to an index on a 0..last track: percent is 0–100, the
  // stops shape indexes its stops.
  const last = props.percent ? 100 : Math.max(0, props.stops.length - 1);
  const index = props.percent
    ? Math.round(Math.min(100, Math.max(0, props.value)))
    : Math.max(0, props.stops.findIndex((s) => s.value === props.value));
  const pct = last === 0 ? 0 : (index / last) * 100;

  const commit = (i: number) => {
    const at = Math.min(last, Math.max(0, i));
    if (props.percent) {
      if (at !== props.value) props.onChange(at);
    } else {
      const stop = props.stops[at];
      if (stop && stop.value !== props.value) props.onChange(stop.value);
    }
  };

  const pick = (clientX: number) => {
    const el = track.current;
    if (!el || last === 0) return;
    const rect = el.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    commit(Math.round(ratio * last));
  };

  const caption = props.percent ? `${index}%` : props.stops[index]?.label;

  return (
    <div className="px-2.5 pt-1.5 pb-2">
      <div
        ref={track}
        role="slider"
        tabIndex={0}
        aria-label={props.ariaLabel}
        aria-valuemin={0}
        aria-valuemax={last}
        aria-valuenow={index}
        aria-valuetext={caption}
        className="focus-ring relative h-7 cursor-pointer touch-none select-none rounded-lg outline-offset-2"
        onPointerDown={(e) => { e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); pick(e.clientX); }}
        onPointerMove={(e) => { if (e.buttons === 1) pick(e.clientX); }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') { e.preventDefault(); commit(index - 1); }
          if (e.key === 'ArrowRight' || e.key === 'ArrowUp') { e.preventDefault(); commit(index + 1); }
        }}
      >
        <div className="absolute top-1/2 left-0 h-1 w-full -translate-y-1/2 rounded-full bg-border" />
        <div className="absolute top-1/2 left-0 h-1 -translate-y-1/2 rounded-full bg-foreground/60 transition-[width] duration-150" style={{ width: `${pct}%` }} />

        {props.percent ? (
          <span className="pointer-events-none absolute top-1/2 -translate-x-1/2 -translate-y-1/2" style={{ left: `${pct}%` }}>
            <span className="block h-3 w-3 rounded-full border-2 border-foreground bg-background" />
          </span>
        ) : (
          props.stops.map((s, i) => {
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
          })
        )}
      </div>
      <div className="mt-1 text-center font-mono text-[10px] text-muted">{caption}</div>
    </div>
  );
}
