import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useAnchoredPosition, useDismiss } from './popover';
import { COLOR_PRESETS } from '../../hooks/useThemeColors';
import { INPUT_INLINE_CLS } from './Input';
import { Button } from './Button';

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;

// One theme token's colour: a swatch that opens the shared popover grammar with
// a preset ramp and a hex field. Never the OS colour dialog. `value` is the
// override ('' = following the theme); `resolved` is the colour painted right
// now, shown in the swatch when no override is set.
export function ColorField({ label, resolved, value, onChange, onReset }: {
  label: string;
  resolved: string;
  value: string;
  onChange: (v: string) => void;
  onReset: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(value);
  const anchor = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const pos = useAnchoredPosition(open, anchor, { minWidth: 224 });
  useDismiss(open, () => setOpen(false), anchor, panel);

  // Seed the hex field from the current override whenever the popover opens.
  useEffect(() => { if (open) setDraft(value); }, [open, value]);

  const custom = Boolean(value);
  const commit = () => { const v = draft.trim(); if (HEX.test(v)) onChange(v.toLowerCase()); };

  return (
    <div className="flex items-center gap-2">
      <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-muted">{label}</span>
      {custom && (
        <button
          type="button"
          onClick={onReset}
          className="shrink-0 cursor-pointer font-mono text-[10px] text-muted transition-colors hover:text-foreground"
        >
          reset
        </button>
      )}
      <button
        ref={anchor}
        type="button"
        aria-label={`${label} colour`}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="focus-ring flex h-6 shrink-0 cursor-pointer items-center gap-1.5 rounded border border-border px-1.5 transition-colors hover:border-foreground/40"
      >
        <span className="h-3.5 w-3.5 rounded-sm border" style={{ background: value || resolved, borderColor: 'var(--color-border)' }} />
        <span className="font-mono text-[10px] text-muted">{custom ? value : 'theme'}</span>
      </button>

      {open && pos && createPortal(
        <div ref={panel} role="dialog" aria-label={`${label} colour`} className="popover-shell p-2" style={{ top: pos.top, bottom: pos.bottom, left: pos.left }}>
          <div className="grid grid-cols-8 gap-1.5">
            {COLOR_PRESETS.map((c) => (
              <button
                key={c}
                type="button"
                aria-label={c}
                onClick={() => { onChange(c); setOpen(false); }}
                className={`h-5 w-5 cursor-pointer rounded border transition-transform duration-150 hover:scale-110 ${value.toLowerCase() === c ? 'ring-2 ring-foreground/50' : ''}`}
                style={{ background: c, borderColor: 'var(--color-border)' }}
              />
            ))}
          </div>
          <div className="mt-2 flex items-center gap-1.5">
            <input
              value={draft}
              spellCheck={false}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') { commit(); setOpen(false); } }}
              placeholder="#rrggbb"
              className={`${INPUT_INLINE_CLS} min-w-0 flex-1`}
            />
            <Button onClick={() => { commit(); setOpen(false); }}>set</Button>
          </div>
        </div>,
        document.body,
      )}
    </div>
  );
}
