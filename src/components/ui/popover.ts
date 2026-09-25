import { useEffect, useRef, useState, type RefObject } from 'react';

// The placement grammar shared by every transient panel (Select, MultiDropdown,
// ContextMenu, Toast triggers, the persona picker). One implementation, so no
// two menus can drift apart:
//   portalled to <body> · viewport-clamped · opens upward when the trigger sits
//   low on screen · dismissed on outside mousedown + Escape.
const GAP = 6;
const EDGE = 8;

export function useAnchoredPosition(
  open: boolean,
  anchor: RefObject<HTMLElement | null>,
  opts?: { matchWidth?: boolean; minWidth?: number },
) {
  const matchWidth = opts?.matchWidth ?? false;
  const minWidth = opts?.minWidth ?? 0;
  const [pos, setPos] = useState<{ top?: number; bottom?: number; left: number; width?: number } | null>(null);

  useEffect(() => {
    if (!open) { setPos(null); return; }
    const update = () => {
      const r = anchor.current?.getBoundingClientRect();
      if (!r) return;
      // No room below and the trigger is past the midline: open upward.
      const openUp = r.top > window.innerHeight * 0.6;
      const left = Math.max(EDGE, Math.min(r.left, window.innerWidth - Math.max(r.width, minWidth) - EDGE));
      setPos({
        ...(openUp ? { bottom: window.innerHeight - r.top + GAP } : { top: r.bottom + GAP }),
        left,
        ...(matchWidth ? { width: r.width } : {}),
      });
    };
    update();
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    return () => {
      window.removeEventListener('resize', update);
      window.removeEventListener('scroll', update, true);
    };
  }, [open, anchor, matchWidth, minWidth]);

  return pos;
}

export function useDismiss(
  open: boolean,
  close: () => void,
  ...refs: (RefObject<HTMLElement | null> | undefined)[]
) {
  const closeRef = useRef(close);
  closeRef.current = close;

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (refs.some((r) => r?.current?.contains(e.target as Node))) return;
      closeRef.current();
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') closeRef.current(); };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
    // refs are stable; only the open state matters here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, ...refs]);
}

// The one row recipe used by every menu, dropdown and context menu.
export const OPTION_ROW =
  'flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left font-mono text-xs text-muted transition-colors hover:bg-border/30 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40';

export const OPTION_ROW_DANGER =
  'flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left font-mono text-xs text-danger transition-colors hover:bg-danger/10 disabled:cursor-not-allowed disabled:opacity-40';
