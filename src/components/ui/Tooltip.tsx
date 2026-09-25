import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

const GAP = 8;

// Immediate label for icon-only controls. Portalled to <body> with fixed
// positioning: an in-tree popup would contribute horizontal overflow under the
// anchor and get clipped by a scrollable ancestor. This replaces every native
// `title` in the app — a surviving native tooltip is a defect.
export function Tooltip({ label, children, side = 'bottom', className = 'inline-flex' }: {
  label: string;
  children: React.ReactNode;
  side?: 'bottom' | 'right';
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLSpanElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number; transform: string } | null>(null);

  const show = () => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    setPos(side === 'right'
      ? { top: r.top + r.height / 2, left: r.right + GAP, transform: 'translateY(-50%)' }
      : { top: r.bottom + GAP, left: r.left + r.width / 2, transform: 'translateX(-50%)' });
  };
  const hide = () => setPos(null);

  // Clamp to the viewport once measured — never let the label run off-screen.
  useEffect(() => {
    if (!pos || !tipRef.current) return;
    const t = tipRef.current.getBoundingClientRect();
    const overRight = t.right - (window.innerWidth - GAP);
    const overLeft = GAP - t.left;
    const shift = overRight > 0 ? -overRight : overLeft > 0 ? overLeft : 0;
    if (shift !== 0) setPos((p) => (p ? { ...p, left: p.left + shift } : p));
  }, [pos]);

  // Anchored to a moving target, a tooltip should just get out of the way.
  useEffect(() => {
    if (!pos) return;
    const onMove = () => setPos(null);
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setPos(null); };
    window.addEventListener('scroll', onMove, true);
    window.addEventListener('resize', onMove);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('scroll', onMove, true);
      window.removeEventListener('resize', onMove);
      window.removeEventListener('keydown', onKey);
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
          ref={tipRef}
          role="tooltip"
          className="pointer-events-none fixed z-[9999] animate-[ip-pop_100ms_var(--ease-panel)_both] rounded border border-border bg-surface px-1.5 py-0.5 text-[9px] leading-none whitespace-nowrap text-muted"
          style={{ top: pos.top, left: pos.left, transform: pos.transform }}
        >
          {label}
        </span>,
        document.body,
      )}
    </>
  );
}
