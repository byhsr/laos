import { useEffect } from 'react';
import { X } from 'lucide-react';
import { IconButton } from './Button';

// A right-docked panel that opens over the surface it belongs to. Same chrome
// grammar as Modal (surface panel, lowercase mono title, close button), but
// anchored to its parent instead of centred — it is a side window, never a
// full-screen view, and it never covers the app chrome. The parent must be
// positioned (`relative`).
export function SidePanel({ title, onClose, children, headerAction }: {
  title: string; onClose: () => void; children: React.ReactNode; headerAction?: React.ReactNode;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div className="absolute inset-0 z-40 flex justify-end">
      <div className="absolute inset-0 bg-black/40" onMouseDown={onClose} />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="side-panel relative flex h-full w-[min(92vw,460px)] flex-col overflow-hidden border-l border-border bg-surface shadow-2xl"
      >
        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border px-4 py-2">
          <span className="min-w-0 flex-1 truncate font-mono text-xs lowercase text-foreground">{title}</span>
          <div className="flex shrink-0 items-center gap-1.5">
            {headerAction}
            <IconButton label="close" onClick={onClose}><X size={14} /></IconButton>
          </div>
        </header>
        <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto p-4">
          {children}
        </div>
      </div>
    </div>
  );
}
