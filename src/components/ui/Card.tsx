import type { HTMLAttributes } from 'react';

// A discrete, repeated item in a list. Panels are flat regions divided by
// borders and never get card chrome; cards are for repeated items only.
export function Card({ className = '', children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div {...rest} className={`rounded-xl border border-border bg-surface px-4 py-3 transition-colors ${className}`}>
      {children}
    </div>
  );
}
