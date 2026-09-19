import { Check } from 'lucide-react';

// On-brand checkbox row — the native control is replaced so config panels match
// the rest of the UI.
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
      className="group flex w-full cursor-pointer items-center gap-2.5 rounded-lg px-2 py-1.5 text-left transition-colors duration-150 hover:bg-panel2 disabled:cursor-not-allowed disabled:opacity-40"
    >
      <span
        className={`grid h-[15px] w-[15px] shrink-0 place-items-center rounded-[5px] border transition-colors duration-150 ${
          checked ? 'border-[var(--green)] bg-[var(--green)]' : 'border-line bg-panel2 group-hover:border-mid'
        }`}
      >
        {checked && <Check size={10} strokeWidth={3.5} className="text-[var(--color-bg)]" />}
      </span>
      <span className="min-w-0 flex-1 truncate text-[12.5px] text-text">{label}</span>
      {hint && <span className="shrink-0 font-mono text-[10px] text-muted">{hint}</span>}
    </button>
  );
}
