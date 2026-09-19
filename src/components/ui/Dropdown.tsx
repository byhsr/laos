import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronRight } from 'lucide-react';

export function Dropdown({ value, options, onChange, placeholder }: {
  value: string; options: { value: string; label: string }[]; onChange: (v: string) => void; placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const selected = options.find((o) => o.value === value);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node) && menuRef.current && !menuRef.current.contains(e.target as Node)) setOpen(false); };
    const onEsc = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onEsc);
    return () => { document.removeEventListener('mousedown', onDoc); document.removeEventListener('keydown', onEsc); };
  }, [open]);

  // Keep the popup glued to the trigger even if the page scrolls or the parent clips.
  const [pos, setPos] = useState<{ top: number; left: number; width: number } | null>(null);
  useEffect(() => {
    if (!open) { setPos(null); return; }
    const update = () => {
      const r = ref.current?.getBoundingClientRect();
      if (r) setPos({ top: r.bottom + 6, left: r.left, width: r.width });
    };
    update();
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    return () => { window.removeEventListener('resize', update); window.removeEventListener('scroll', update, true); };
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <button type="button" className="group flex w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-hairline bg-panel2 px-3 py-1.5 text-left text-[12.5px] text-text transition-colors duration-150 hover:border-mid" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span>{selected ? selected.label : (placeholder ?? 'Select…')}</span>
        <ChevronRight size={13} className="rotate-90 text-muted transition-transform duration-150 group-aria-expanded:-rotate-90" />
      </button>
      {open && pos && createPortal(
        <div ref={menuRef} className="glass-strong fixed z-[200] grid max-h-[260px] animate-[dropdown-in_140ms_ease-out] gap-0.5 overflow-y-auto rounded-xl border border-hairline p-1 shadow-float" style={{ top: pos.top, left: pos.left, width: pos.width }}>
          {options.map((o) => (
            <button
              key={o.value} type="button"
              className={`w-full cursor-pointer rounded-md border-0 px-2.5 py-1.5 text-left text-[12.5px] text-text transition-colors duration-150 ${o.value === value ? 'bg-line font-semibold' : 'bg-transparent hover:bg-line'}`}
              onClick={() => { onChange(o.value); setOpen(false); }}
            >
              {o.label}
            </button>
          ))}
        </div>,
        document.body
      )}
    </div>
  );
}
