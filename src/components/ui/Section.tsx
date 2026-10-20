import { useState, type ReactNode } from 'react';
import { ChevronRight } from 'lucide-react';

// The one disclosure grammar: an uppercase label, an optional one-line summary,
// and a chevron that rotates open. Collapsed by default so chrome stays quiet
// and nothing is hidden — just quiet.
export function Section({ title, summary, defaultOpen = false, className = 'border-t border-border pt-4', children }: {
  title: string; summary?: string; defaultOpen?: boolean; className?: string; children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className={className}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="focus-ring flex w-full cursor-pointer items-center gap-2 border-0 bg-transparent text-left"
      >
        <span className="shrink-0 font-mono text-[10px] tracking-wider text-muted uppercase">{title}</span>
        <span className="min-w-0 flex-1 truncate text-right font-mono text-[10px] text-muted">{summary}</span>
        <ChevronRight size={13} className={`shrink-0 text-muted transition-transform duration-150 ${open ? 'rotate-90' : ''}`} />
      </button>
      {open && <div className="mt-3.5">{children}</div>}
    </div>
  );
}
