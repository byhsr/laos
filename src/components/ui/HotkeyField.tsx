import { useEffect, useState } from 'react';
import { INPUT_CLS } from './Input';

// Captures the next key chord and stores it as a Tauri accelerator string. The
// backend parses it, so this only has to produce the conventional names.
function acceleratorKey(e: KeyboardEvent): string | null {
  const code = e.code;
  if (code === 'Space') return 'Space';
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  const named = ['Enter', 'Tab', 'Backspace', 'Delete', 'Home', 'End', 'PageUp', 'PageDown', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];
  return named.includes(code) ? code : null;
}

export function prettyAccelerator(value: string): string {
  if (!value) return '';
  const ctrl = navigator.userAgent.includes('Mac') ? 'Cmd' : 'Ctrl';
  return value.replace(/CommandOrControl/g, ctrl).replace(/Control/g, ctrl);
}

export function HotkeyField({ value, onChange }: { value: string; onChange: (accelerator: string) => void }) {
  const [listening, setListening] = useState(false);

  useEffect(() => {
    if (!listening) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === 'Escape') { setListening(false); return; }
      // Wait for a non-modifier key so Ctrl+Shift+Space isn't captured as Ctrl.
      if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return;
      const parts: string[] = [];
      if (e.ctrlKey || e.metaKey) parts.push('CommandOrControl');
      if (e.altKey) parts.push('Alt');
      if (e.shiftKey) parts.push('Shift');
      const key = acceleratorKey(e);
      if (!key) return;
      onChange([...parts, key].join('+'));
      setListening(false);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [listening, onChange]);

  return (
    <div className="flex items-center gap-2">
      <button
        type="button"
        className={`${INPUT_CLS} cursor-pointer text-left`}
        onClick={() => setListening(true)}
      >
        {listening ? 'press a shortcut…' : (prettyAccelerator(value) || 'disabled')}
      </button>
      {value && !listening && (
        <button
          type="button"
          className="shrink-0 cursor-pointer font-mono text-[10px] text-muted underline hover:text-foreground"
          onClick={() => onChange('')}
        >
          disable
        </button>
      )}
    </div>
  );
}
