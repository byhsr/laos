import { useEffect, useRef, useState } from 'react';
import { ChevronRight } from 'lucide-react';

export function MultiDropdown({ values, options, onChange, placeholder }: {
  values: string[]; options: { value: string; label: string }[]; onChange: (v: string[]) => void; placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const selected = options.filter((o) => values.includes(o.value));

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const onEsc = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onEsc);
    return () => { document.removeEventListener('mousedown', onDoc); document.removeEventListener('keydown', onEsc); };
  }, [open]);

  const toggle = (v: string) => {
    onChange(values.includes(v) ? values.filter((x) => x !== v) : [...values, v]);
  };

  return (
    <div className="relative" ref={ref}>
      <button type="button" className="group flex w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-line bg-panel2 px-3 py-2.5 text-left text-[13px] text-text hover:border-mid" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="overflow-hidden text-ellipsis whitespace-nowrap">
          {selected.length === 0 ? (placeholder ?? 'Select…') : selected.map((s) => s.label).join(', ')}
        </span>
        <ChevronRight size={13} className="rotate-90 text-muted transition-transform duration-150 group-aria-expanded:-rotate-90" />
      </button>
      {open && (
        <div className="absolute top-[calc(100%+6px)] left-0 right-0 z-30 grid max-h-[260px] animate-[dropdown-in_140ms_ease-out] gap-0.5 overflow-y-auto rounded-lg border border-line bg-panel2 p-[5px] shadow-[0_14px_34px_#000a]">
          {options.map((o) => {
            const checked = values.includes(o.value);
            return (
              <button key={o.value} type="button" className={`w-full cursor-pointer rounded-[5px] border-0 px-3 py-2.5 text-left text-[13px] text-text ${checked ? 'bg-line font-semibold' : 'bg-transparent hover:bg-line'}`} onClick={() => toggle(o.value)}>
                <span className={`mr-2 inline-grid h-[15px] w-[15px] place-items-center rounded border border-mid text-[10px] ${checked ? 'border-[var(--green)] bg-[var(--green)] text-[#09090b]' : 'bg-transparent text-transparent'}`}>{checked ? '✓' : ''}</span>
                {o.label}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
