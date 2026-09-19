import { Fragment, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, MoreHorizontal } from 'lucide-react';
import type { ReactNode } from 'react';

export type MenuItem = {
  key: string;
  label: string;
  icon?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  active?: boolean;
  dividerBefore?: boolean;
  onSelect: () => void;
};

// Three-dot overflow menu. Rendered through a portal with a high z-index so it
// can never be buried under the chat or clipped by a scroll container.
export function ContextMenu({ items, title = 'More actions' }: { items: MenuItem[]; title?: string }) {
  const [open, setOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; right: number } | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (btnRef.current?.contains(e.target as Node)) return;
      if (menuRef.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    const onEsc = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onEsc);
    return () => { document.removeEventListener('mousedown', onDoc); document.removeEventListener('keydown', onEsc); };
  }, [open]);

  // Keep the popup glued to the trigger even if the page scrolls or resizes.
  useEffect(() => {
    if (!open) { setPos(null); return; }
    const update = () => {
      const r = btnRef.current?.getBoundingClientRect();
      if (r) setPos({ top: r.bottom + 6, right: window.innerWidth - r.right });
    };
    update();
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    return () => { window.removeEventListener('resize', update); window.removeEventListener('scroll', update, true); };
  }, [open]);

  if (items.length === 0) return null;

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        title={title}
        className={`grid h-6 w-6 cursor-pointer place-items-center rounded-md border-0 bg-transparent transition-colors duration-150 ${open ? 'bg-line text-text' : 'text-muted hover:bg-line hover:text-text'}`}
        onClick={() => setOpen((o) => !o)}
      >
        <MoreHorizontal size={14} />
      </button>

      {open && pos && createPortal(
        <div
          ref={menuRef}
          role="menu"
          className="glass-strong fixed z-[200] min-w-[196px] animate-[dropdown-in_140ms_ease-out] rounded-xl border border-hairline p-1.5 shadow-float"
          style={{ top: pos.top, right: pos.right }}
        >
          {items.map((it) => (
            <Fragment key={it.key}>
              {it.dividerBefore && <div className="my-1 h-px bg-line" />}
              <button
                type="button"
                role="menuitem"
                disabled={it.disabled}
                className={`flex w-full cursor-pointer items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-[12.5px] transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-40 ${it.danger ? 'text-[#f87171] hover:bg-[#f87171]/10' : 'text-text hover:bg-line'}`}
                onClick={() => { setOpen(false); it.onSelect(); }}
              >
                {it.icon}
                <span className="flex-1 truncate">{it.label}</span>
                {it.active && <Check size={12} className="shrink-0 text-[var(--green)]" />}
              </button>
            </Fragment>
          ))}
        </div>,
        document.body
      )}
    </>
  );
}

// Same menu, opened at an arbitrary point (right-click) instead of a trigger
// button. Clamped to the viewport once measured.
export function ContextMenuAt({ items, x, y, onClose }: { items: MenuItem[]; x: number; y: number; onClose: () => void }) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useEffect(() => {
    const onDoc = (e: MouseEvent) => { if (!menuRef.current?.contains(e.target as Node)) onClose(); };
    const onEsc = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onEsc);
    return () => { document.removeEventListener('mousedown', onDoc); document.removeEventListener('keydown', onEsc); };
  }, [onClose]);

  useEffect(() => {
    const el = menuRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      top: Math.max(8, Math.min(y, window.innerHeight - r.height - 8)),
      left: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
    });
  }, [x, y]);

  if (items.length === 0) return null;

  return createPortal(
    <div
      ref={menuRef}
      role="menu"
      className="glass-strong fixed z-[200] min-w-[196px] animate-[dropdown-in_140ms_ease-out] rounded-xl border border-hairline p-1.5 shadow-float"
      style={{ top: pos?.top ?? y, left: pos?.left ?? x, visibility: pos ? 'visible' : 'hidden' }}
    >
      {items.map((it) => (
        <Fragment key={it.key}>
          {it.dividerBefore && <div className="my-1 h-px bg-line" />}
          <button
            type="button"
            role="menuitem"
            disabled={it.disabled}
            className={`flex w-full cursor-pointer items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-[12.5px] transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-40 ${it.danger ? 'text-[#f87171] hover:bg-[#f87171]/10' : 'text-text hover:bg-line'}`}
            onClick={() => { onClose(); it.onSelect(); }}
          >
            {it.icon}
            <span className="flex-1 truncate">{it.label}</span>
            {it.active && <Check size={12} className="shrink-0 text-[var(--green)]" />}
          </button>
        </Fragment>
      ))}
    </div>,
    document.body,
  );
}
