import type { InputHTMLAttributes, TextareaHTMLAttributes } from 'react';

// Field shells. Chrome — every label, menu, count and input value — is mono;
// only prose content (an agent's objective, a document body) is sans.

/** Standard field. */
export const INPUT_CLS =
  'w-full rounded border border-border bg-background px-2 py-1 font-mono text-xs text-foreground outline-none transition-colors placeholder:text-muted/50 focus:border-foreground/40';

/** Compact field for sidebar / inline chrome. */
export const INPUT_INLINE_CLS =
  'rounded border border-border bg-background px-1 py-0.5 font-mono text-[11px] text-foreground outline-none transition-colors placeholder:text-muted/50 focus:border-foreground/40';

/** Prose editor — content, not chrome. */
export const PROSE_CLS =
  'w-full resize-none rounded border border-border bg-background px-3 py-2 text-[15px] leading-[1.65] text-foreground outline-none transition-colors placeholder:text-muted/50 focus:border-foreground/40';

/** Section / group label. Uppercase, tracked, muted, mono. */
export const GROUP_LABEL_CLS = 'font-mono text-[10px] tracking-wider text-muted uppercase';

/** Form field label. */
export const FIELD_LABEL_CLS = 'mb-1.5 block font-mono text-[10px] tracking-wider text-muted uppercase';

export function Input({ className = '', ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...rest} className={`${INPUT_CLS} ${className}`} />;
}

export function Textarea({ className = '', ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea {...rest} className={`${INPUT_CLS} ${className}`} />;
}
