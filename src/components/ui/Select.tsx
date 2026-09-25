import { useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronDown } from 'lucide-react';
import { OPTION_ROW, useAnchoredPosition, useDismiss } from './popover';

export type SelectOption = { value: string; label: string; group?: string };

// The one dropdown in the app. Replaces every native <select>: same control
// shell as a button, same portalled popover shell as every other transient
// panel, dismiss on outside mousedown + Escape, opens upward when low.
export function Select({ value, options, onChange, placeholder = 'select…' }: {
  value: string;
  options: SelectOption[];
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const pos = useAnchoredPosition(open, anchor, { matchWidth: true, minWidth: 200 });
  useDismiss(open, () => setOpen(false), anchor, panel);

  const selected = options.find((o) => o.value === value);

  return (
    <div className="relative" ref={anchor}>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="focus-ring flex h-[30px] w-full cursor-pointer items-center justify-between gap-2 rounded-lg border border-border bg-surface px-2.5 text-left font-mono text-[11px] text-foreground transition-colors hover:bg-background"
      >
        <span className={`min-w-0 flex-1 truncate ${selected ? '' : 'text-muted'}`}>
          {selected ? selected.label : placeholder}
        </span>
        <ChevronDown size={13} className={`shrink-0 text-muted transition-transform duration-150 ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && pos && createPortal(
        <div
          ref={panel}
          role="listbox"
          className="popover-shell max-h-[260px] overflow-x-hidden overflow-y-auto p-1.5"
          style={{ top: pos.top, bottom: pos.bottom, left: pos.left, width: pos.width }}
        >
          {options.length === 0 && (
            <p className="m-0 px-2.5 py-1.5 font-mono text-xs lowercase text-muted">no options</p>
          )}
          {options.map((o) => (
            <button
              key={o.value}
              type="button"
              role="option"
              aria-selected={o.value === value}
              className={`${OPTION_ROW} ${o.value === value ? 'text-foreground' : ''}`}
              onClick={() => { onChange(o.value); setOpen(false); }}
            >
              <span className="min-w-0 flex-1 truncate">{o.label}</span>
            </button>
          ))}
        </div>,
        document.body,
      )}
    </div>
  );
}
