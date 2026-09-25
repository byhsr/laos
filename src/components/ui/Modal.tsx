import { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';
import { IconButton } from './Button';

// The single overlay shell. Every full overlay in the app is built from this:
// dim backdrop at rgba(0,0,0,0.6), one surface panel, a lowercase mono title
// bar, and an internally scrolling body. If two overlays differ in chrome, that
// is a bug.
//
// Portalled so no ancestor transform or overflow can displace or clip it.
export function Modal({ title, onClose, children, headerAction, width, height }: {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  headerAction?: React.ReactNode;
  width?: string;
  height?: string;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  return createPortal(
    <>
      <div className="overlay-backdrop" onMouseDown={onClose} />
      <div
        className="overlay-panel"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        style={{ ...(width ? { width } : {}), ...(height ? { height } : {}) }}
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
    </>,
    document.body,
  );
}
