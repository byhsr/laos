import { Fragment, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronRight } from 'lucide-react';

export function MultiDropdown({ values, options, onChange, placeholder }: {
  values: string[]; options: { value: string; label: string; group?: string }[]; onChange: (v: string[]) => void; placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const selected = options.filter((o) => values.includes(o.value));

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

  const toggle = (v: string) => {
    onChange(values.includes(v) ? values.filter((x) => x !== v) : [...values, v]);
  };

  return (
    <div className="relative" ref={ref}>
      <button type="button" className="group flex w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-hairline bg-panel2 px-3 py-1.5 text-left text-[12.5px] text-text transition-colors duration-150 hover:border-mid" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="overflow-hidden text-ellipsis whitespace-nowrap">
          {selected.length === 0 ? (placeholder ?? 'Select…') : selected.map((s) => s.label).join(', ')}
        </span>
        <ChevronRight size={13} className="rotate-90 text-muted transition-transform duration-150 group-aria-expanded:-rotate-90" />
      </button>
      {open && pos && createPortal(
        <div ref={menuRef} className="glass-strong fixed z-[200] grid max-h-[260px] animate-[dropdown-in_140ms_ease-out] gap-0.5 overflow-y-auto rounded-xl border border-hairline p-1 shadow-float" style={{ top: pos.top, left: pos.left, width: pos.width }}>
          {options.map((o, i) => {
            const checked = values.includes(o.value);
            // Options arrive pre-grouped, so a header is just the first row of
            // each run of same-group options.
            const header = o.group && o.group !== options[i - 1]?.group ? o.group : null;
            return (
              <Fragment key={o.value}>
                {header && <div className="px-2.5 pt-2 pb-1 font-mono text-[10px] tracking-[1px] text-muted uppercase">{header}</div>}
                <button type="button" className={`w-full cursor-pointer rounded-md border-0 px-2.5 py-1.5 text-left text-[12.5px] text-text transition-colors duration-150 ${checked ? 'bg-line font-semibold' : 'bg-transparent hover:bg-line'}`} onClick={() => toggle(o.value)}>
                  <span className={`mr-2 inline-grid h-3.5 w-3.5 place-items-center rounded-[4px] border border-mid text-[10px] ${checked ? 'border-[var(--green)] bg-[var(--green)] text-[#09090b]' : 'bg-transparent text-transparent'}`}>{checked ? '✓' : ''}</span>
                  {o.label}
                </button>
              </Fragment>
            );
          })}
        </div>,
        document.body
      )}
    </div>
  );
}
