import { useState } from 'react';

// Immediate label for icon-only controls. Native `title` tooltips are slow and
// inconsistent inside the webview, so the shell uses this instead.
export function Tooltip({ label, children, side = 'bottom', className = 'inline-flex' }: {
  label: string;
  children: React.ReactNode;
  side?: 'bottom' | 'right';
  className?: string;
}) {
  const [show, setShow] = useState(false);

  return (
    <span
      className={`app-no-drag relative ${className}`}
      onMouseEnter={() => setShow(true)}
      onMouseLeave={() => setShow(false)}
      onFocus={() => setShow(true)}
      onBlur={() => setShow(false)}
    >
      {children}
      {show && (
        <span
          role="tooltip"
          className={`glass-strong pointer-events-none absolute z-[300] whitespace-nowrap rounded-md border border-hairline px-2 py-1 text-[10.5px] leading-none text-text shadow-float ${
            side === 'right'
              ? 'top-1/2 left-[calc(100%+7px)] -translate-y-1/2'
              : 'top-[calc(100%+7px)] left-1/2 -translate-x-1/2'
          }`}
        >
          {label}
        </span>
      )}
    </span>
  );
}
