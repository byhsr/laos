import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { Tooltip } from './Tooltip';

// One control shell for the whole app, defined once in styles.css so a button
// can never drift from the rest of the chrome. Hierarchy is brightness, not
// hue: `primary` is foreground text, the default is muted, and `accent` is the
// single place the accent fills a control (confirm / emphasis CTAs only).
type Variant = 'default' | 'primary' | 'accent' | 'danger';

const VARIANT: Record<Variant, string> = {
  default: 'secondary',
  primary: 'primary',
  accent: 'accent',
  danger: 'danger',
};

export function Button({ variant = 'default', icon, children, className = '', ...rest }: {
  variant?: Variant;
  icon?: ReactNode;
  children?: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button type="button" {...rest} className={`${VARIANT[variant]} ${className}`}>
      {icon}
      {children}
    </button>
  );
}

// Icon-only control. Renders its label through the shared Tooltip, so a native
// `title` can never creep back in.
export function IconButton({ label, children, className = '', ...rest }: {
  label?: string;
  children: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>) {
  const button = (
    <button type="button" {...rest} aria-label={label} className={`control icon-btn ${className}`}>
      {children}
    </button>
  );
  return label ? <Tooltip label={label}>{button}</Tooltip> : button;
}

// A chrome control that is not a Button: bare, icon-led, tight hit area.
export const GHOST_BTN =
  'grid shrink-0 cursor-pointer place-items-center rounded-lg border-0 bg-transparent p-1.5 text-muted transition-colors hover:bg-background hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40 focus-ring';
