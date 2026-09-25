import { useRef, useState } from 'react';
import { Send } from 'lucide-react';
import { IconButton } from '../ui/Button';
import { ReasoningPicker } from '../ui/ReasoningPicker';
import { ConfirmDialog } from '../ui/ConfirmDialog';

// The chat composer, shared by the lead agent and every agent window so the two
// can't drift. It floats over the message log on a gradient fade rather than
// taking an in-flow row, and grows with its content up to a cap.
//
// Two rows: what you're writing, then what you can do with it — the send action
// and the reasoning picker for the model this chat is running on.
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
    <div className="absolute right-0 bottom-0 left-0 bg-gradient-to-t from-background via-background/85 to-transparent p-3 pt-6">
      <div className="relative mx-auto w-full max-w-[720px]">
        {/* Confirmation sits where the user is looking — above what they're typing. */}
        <div className="absolute right-0 bottom-[calc(100%+8px)] left-0 z-30">
          <ConfirmDialog />
        </div>

        <div className="flex flex-col rounded-lg border border-border bg-surface transition-colors focus-within:border-foreground/40">
          <textarea
            ref={ref}
            rows={1}
            value={input}
            onChange={onChange}
            onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void submit(); } }}
            placeholder={placeholder}
            className="max-h-[160px] min-h-[38px] w-full resize-none border-0 bg-transparent px-3.5 pt-2.5 pb-1 font-mono text-xs leading-relaxed text-foreground outline-none placeholder:text-muted/60"
          />
          <div className="flex items-center justify-end gap-2 px-2.5 pt-1 pb-2.5">
            <ReasoningPicker modelId={modelId} />
            <IconButton label="send" className="h-[30px] w-[30px] shrink-0" disabled={busy || !input.trim()} onClick={submit}>
              <Send size={13} />
            </IconButton>
          </div>
        </div>
      </div>
    </div>
  );
}
