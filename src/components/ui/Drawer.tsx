import { useCallback, useState } from 'react';
import { X } from 'lucide-react';

export function Drawer({ title, onClose, children, initialWidth, resizable, headerAction }: {
  title: string; onClose: () => void; children: React.ReactNode; initialWidth?: number; resizable?: boolean; headerAction?: React.ReactNode;
}) {
  const [width, setWidth] = useState(initialWidth);

  const onDragStart = useCallback((e: React.PointerEvent) => {
    if (!resizable) return;
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = width ?? Math.min(520, window.innerWidth * 0.92);
    const onMove = (ev: PointerEvent) => {
      const next = Math.min(Math.max(startWidth + (startX - ev.clientX), 320), window.innerWidth - 80);
      setWidth(next);
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      document.body.style.cursor = '';
    };
    document.body.style.cursor = 'col-resize';
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  }, [resizable, width]);

  const style: React.CSSProperties = resizable
    ? { width: width ?? Math.min(520, window.innerWidth * 0.92) }
    : {};

  return (
    <>
      <div className="absolute inset-0 z-20 animate-[overlay-in_180ms_ease-out] bg-black/40 backdrop-blur-[2px]" onClick={onClose} />
      <div
        className={`glass-strong absolute top-0 right-0 bottom-0 z-[21] flex max-w-[95%] animate-[drawer-in_220ms_cubic-bezier(0.16,1,0.3,1)] flex-col rounded-l-3xl border-l border-hairline shadow-float ${resizable ? '' : 'w-[min(520px,92vw)]'}`}
        role="dialog"
        aria-label={title}
        style={style}
      >
        {resizable && (
          <div
            className="absolute top-0 bottom-0 left-0 z-10 w-1 cursor-col-resize bg-transparent transition-colors hover:bg-white/30"
            onPointerDown={onDragStart}
            title="Drag to resize"
          />
        )}
        <div className="flex h-[64px] flex-none items-center justify-between gap-3 border-b border-hairline px-6">
          <b style={{ fontSize: 15 }} className="truncate">{title}</b>
          <div className="flex shrink-0 items-center gap-2">
            {headerAction}
            <button className="secondary" onClick={onClose}><X size={13} />Close</button>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto p-6">{children}</div>
      </div>
    </>
  );
}
