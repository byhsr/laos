import { useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useAnchoredPosition, useDismiss } from './popover';
import { PEN_COLORS, penColor } from '../../pen';
import { useScribble } from '../../hooks/useScribble';

// Pen-colour picker: a swatch trigger and a popover of the palette. The swatch
// colours are read straight from theme tokens, so the palette is never
// duplicated here.
export function PenPicker({ className = '' }: { className?: string }) {
  const [open, setOpen] = useState(false);
  const pen = useScribble((s) => s.pen);
  const setPen = useScribble((s) => s.setPen);
  const anchor = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const pos = useAnchoredPosition(open, anchor, { minWidth: 176 });
  useDismiss(open, () => setOpen(false), anchor, panel);

  return (
    <div className="relative" ref={anchor}>
      <button
        type="button"
        aria-label="pen colour"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className={className}
      >
        <span
          className="block h-3.5 w-3.5 rounded-full border"
          style={{ background: penColor(pen), borderColor: 'var(--color-border)' }}
        />
      </button>

      {open && pos && createPortal(
        <div
          ref={panel}
          role="listbox"
          aria-label="pen colour"
          className="popover-shell flex items-center gap-1.5 p-2"
          style={{ top: pos.top, bottom: pos.bottom, left: pos.left }}
        >
          {PEN_COLORS.map((c) => (
            <button
              key={c.key}
              type="button"
              role="option"
              aria-selected={c.key === pen}
              aria-label={c.label}
              title={c.label}
              onClick={() => { setPen(c.key); setOpen(false); }}
              className={`h-5 w-5 shrink-0 cursor-pointer rounded-full border transition-transform duration-150 hover:scale-110 ${
                c.key === pen ? 'ring-2 ring-foreground/50' : ''
              }`}
              style={{ background: penColor(c.key), borderColor: 'var(--color-border)' }}
            />
          ))}
        </div>,
        document.body,
      )}
    </div>
  );
}
