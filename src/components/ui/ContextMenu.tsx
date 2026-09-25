import { Fragment, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, MoreHorizontal } from 'lucide-react';
import type { ReactNode } from 'react';
import { Tooltip } from './Tooltip';
import { OPTION_ROW, OPTION_ROW_DANGER, useAnchoredPosition, useDismiss } from './popover';

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

const PANEL = 'popover-shell min-w-[168px] overflow-x-hidden overflow-y-auto p-1.5';

function MenuRows({ items, onDone }: { items: MenuItem[]; onDone: () => void }) {
  return items.map((it) => (
    <Fragment key={it.key}>
      {it.dividerBefore && <div className="my-1 h-px bg-border" />}
      <button
        type="button"
        role="menuitem"
        disabled={it.disabled}
        className={`${it.danger ? OPTION_ROW_DANGER : OPTION_ROW} ${it.active ? 'text-foreground' : ''}`}
        onClick={() => { onDone(); it.onSelect(); }}
      >
        {it.icon}
        <span className="min-w-0 flex-1 truncate">{it.label}</span>
        {it.active && <Check size={12} className="shrink-0 text-foreground" />}
      </button>
    </Fragment>
  ));
}

// Three-dot overflow menu. Portalled with the shared popover shell so it can
// never be buried under the chat or clipped by a scroll container.
export function ContextMenu({ items, title = 'more' }: { items: MenuItem[]; title?: string }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const pos = useAnchoredPosition(open, anchor, { minWidth: 168 });
  useDismiss(open, () => setOpen(false), anchor, panel);

  if (items.length === 0) return null;

  return (
    <>
      <Tooltip label={title}>
        <button
          ref={anchor}
          type="button"
          aria-haspopup="menu"
          aria-expanded={open}
          aria-label={title}
          className={`focus-ring grid h-[26px] w-[26px] cursor-pointer place-items-center rounded-lg transition-colors ${
            open ? 'bg-background text-foreground' : 'text-muted hover:bg-background hover:text-foreground'
          }`}
          onClick={() => setOpen((o) => !o)}
        >
          <MoreHorizontal size={14} />
        </button>
      </Tooltip>

      {open && pos && createPortal(
        <div ref={panel} role="menu" className={PANEL} style={{ top: pos.top, bottom: pos.bottom, left: pos.left }}>
          <MenuRows items={items} onDone={() => setOpen(false)} />
        </div>,
        document.body,
      )}
    </>
  );
}

// Same panel, opened at an arbitrary point (right-click) instead of a trigger
// button. Clamped to the viewport once measured.
export function ContextMenuAt({ items, x, y, onClose }: { items: MenuItem[]; x: number; y: number; onClose: () => void }) {
  const panel = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    const onDown = (e: MouseEvent) => { if (!panel.current?.contains(e.target as Node)) onClose(); };
    document.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
    };
  }, [onClose]);

  useEffect(() => {
    const r = panel.current?.getBoundingClientRect();
    if (!r) return;
    setPos({
      top: Math.max(8, Math.min(y, window.innerHeight - r.height - 8)),
      left: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
    });
  }, [x, y]);

  if (items.length === 0) return null;

  return createPortal(
    <div
      ref={panel}
      role="menu"
      className={PANEL}
      style={{ top: pos?.top ?? y, left: pos?.left ?? x, visibility: pos ? 'visible' : 'hidden' }}
    >
      <MenuRows items={items} onDone={onClose} />
    </div>,
    document.body,
  );
}
