import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

// Immediate label for icon-only controls. Rendered through a portal with fixed
// positioning: an absolutely-positioned popup inside a scrollable panel (the
// agent list) would otherwise contribute horizontal scroll overflow under the
// row, and get clipped by that panel's bounds.
export function Tooltip({ label, children, side = 'bottom', className = 'inline-flex' }: {
  label: string;
  children: React.ReactNode;
  side?: 'bottom' | 'right';
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number; transform: string } | null>(null);

  const show = () => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    setPos(side === 'right'
      ? { top: r.top + r.height / 2, left: r.right + 7, transform: 'translateY(-50%)' }
      : { top: r.bottom + 7, left: r.left + r.width / 2, transform: 'translateX(-50%)' });
  };
  const hide = () => setPos(null);

  // Anchored to a moving target, a tooltip should just get out of the way.
  useEffect(() => {
    if (!pos) return;
    const onMove = () => setPos(null);
    window.addEventListener('scroll', onMove, true);
    window.addEventListener('resize', onMove);
    return () => {
      window.removeEventListener('scroll', onMove, true);
      window.removeEventListener('resize', onMove);
    };
  }, [pos]);

  return (
    <>
      <span
        ref={ref}
        className={`app-no-drag relative ${className}`}
        onMouseEnter={show}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
      >
        {children}
      </span>
      {pos && createPortal(
        <span
          role="tooltip"
          className="glass-strong pointer-events-none fixed z-[300] whitespace-nowrap rounded-md border border-hairline px-2 py-1 text-[10.5px] leading-none text-text shadow-float"
          style={{ top: pos.top, left: pos.left, transform: pos.transform }}
        >
          {label}
        </span>,
        document.body,
      )}
    </>
  );
}
