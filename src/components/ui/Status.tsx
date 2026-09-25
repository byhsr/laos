import { Ban, Check, Clock, Loader2, X } from 'lucide-react';

// Status is one grammar everywhere: a glyph plus brightness. Hue appears only
// for failure, so a status reads identically in every theme and never competes
// with the accent.
export function statusClass(status: string) {
  if (status === 'failed') return 'text-danger';
  if (status === 'running') return 'text-foreground';
  if (status === 'completed') return 'text-foreground/70';
  return 'text-muted';
}

export function StatusGlyph({ status, size = 11 }: { status: string; size?: number }) {
  if (status === 'running') return <Loader2 size={size} className="animate-spin" />;
  if (status === 'completed') return <Check size={size} />;
  if (status === 'failed') return <X size={size} />;
  if (status === 'cancelled') return <Ban size={size} />;
  return <Clock size={size} />;
}

export function StatusTag({ status, className = '' }: { status: string; className?: string }) {
  return (
    <span className={`inline-flex shrink-0 items-center gap-1 font-mono text-[10px] lowercase ${statusClass(status)} ${className}`}>
      <StatusGlyph status={status} />
      {status}
    </span>
  );
}
