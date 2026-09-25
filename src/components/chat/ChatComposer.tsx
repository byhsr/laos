import { useRef, useState } from 'react';
import { Send } from 'lucide-react';
import { IconButton } from '../ui/Button';
import { ReasoningPicker } from '../ui/ReasoningPicker';
import { ConfirmDialog } from '../ui/ConfirmDialog';

// The chat composer, shared by the lead agent and every agent window so the two
// can't drift. It floats over the message log on a gradient fade rather than
// taking an in-flow row, and grows with its content up to a cap.
export function ChatComposer({ placeholder, busy, modelId, onSend }: {
  placeholder: string;
  busy: boolean;
  modelId: string;
  onSend: (text: string) => void | Promise<void>;
}) {
  const [input, setInput] = useState('');
  const ref = useRef<HTMLTextAreaElement>(null);

  // Auto-grow to the content, capped so a long message scrolls instead.
  const onChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    e.target.style.height = 'auto';
    e.target.style.height = `${Math.min(e.target.scrollHeight, 160)}px`;
  };

  const submit = async () => {
    if (!input.trim() || busy) return;
    const text = input.trim();
    setInput('');
    if (ref.current) ref.current.style.height = 'auto';
    await onSend(text);
  };

  return (
    <div className="absolute right-0 bottom-0 left-0 flex items-end gap-2.5 bg-gradient-to-t from-background via-background/85 to-transparent p-3 pt-6">
      {/* Confirmation sits where the user is looking — above what they're typing. */}
      <div className="absolute right-3 bottom-[calc(100%+8px)] left-3 z-30">
        <ConfirmDialog />
      </div>
      <ReasoningPicker modelId={modelId} />
      <textarea
        ref={ref}
        rows={1}
        value={input}
        onChange={onChange}
        onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void submit(); } }}
        placeholder={placeholder}
        className="max-h-[160px] min-h-[42px] min-w-0 flex-1 resize-none rounded border border-border bg-surface px-3.5 py-2.5 font-mono text-xs leading-relaxed text-foreground outline-none transition-colors placeholder:text-muted/60 focus:border-foreground/40"
      />
      <IconButton label="send" className="h-[42px] w-[42px] shrink-0" disabled={busy || !input.trim()} onClick={submit}>
        <Send size={14} />
      </IconButton>
    </div>
  );
}
