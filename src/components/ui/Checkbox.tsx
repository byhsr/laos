import { Check } from 'lucide-react';

// On-brand checkbox row — the native control is replaced so config panels match
// the rest of the UI. Selection is neutral (foreground fill), never the accent.
export function Checkbox({ checked, onChange, label, hint, disabled = false }: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="focus-ring group flex w-full cursor-pointer items-center gap-2.5 rounded-lg px-2 py-1.5 text-left transition-colors hover:bg-background disabled:cursor-not-allowed disabled:opacity-40"
    >
      <span
        className={`grid h-3.5 w-3.5 shrink-0 place-items-center rounded-[3px] border transition-colors ${
          checked ? 'border-foreground/80 bg-foreground/80' : 'border-border bg-background group-hover:border-foreground/40'
        }`}
      >
        {checked && <Check size={10} strokeWidth={3.5} className="text-background" />}
      </span>
      <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{label}</span>
      {hint && <span className="shrink-0 font-mono text-[10px] text-muted">{hint}</span>}
    </button>
  );
}
