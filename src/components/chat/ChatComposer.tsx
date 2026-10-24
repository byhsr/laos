import { useRef, useState } from 'react';
import { CircleStop, FileText, Paperclip, Pause, Send, Trash2, X } from 'lucide-react';
import { IconButton } from '../ui/Button';
import { ContextMenu } from '../ui/ContextMenu';
import { ComposerSettings } from './ComposerSettings';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import type { Attachment, Reasoning } from '../../types';

// The chat composer, shared by the lead agent and every agent window so the two
// can't drift. It floats over the message log on a gradient fade rather than
// taking an in-flow row, and grows with its content up to a cap.
//
// Two rows: what you're writing, then what you can do with it — the "+" that
// holds every per-turn setting (model, reasoning), the paperclip, and the send
// action. Media can be dropped on it, pasted into it, or picked with the clip;
// whatever arrives rides with the turn.
const MAX_FILES = 4;
const MAX_BYTES = 6 * 1024 * 1024;

// Reads a dropped, pasted or chosen file into the shape the backend wants: base64
// without the `data:` prefix. Returns null when it is too big to send.
function readAttachment(file: File): Promise<Attachment | null> {
  return new Promise((resolve) => {
    if (file.size > MAX_BYTES) { resolve(null); return; }
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result ?? '');
      const comma = url.indexOf(',');
      if (comma < 0) { resolve(null); return; }
      const meta = url.slice(5, url.indexOf(';') > 0 ? url.indexOf(';') : comma);
      const mime = meta && !meta.startsWith(';') ? meta : (file.type || 'application/octet-stream');
      resolve({ name: file.name || 'pasted-image', mime, data: url.slice(comma + 1) });
    };
    reader.onerror = () => resolve(null);
    reader.readAsDataURL(file);
  });
}

export function ChatComposer({ placeholder, busy, modelId, reasoning, onSend, onModelChange, onReasoningChange, onCancel }: {
  placeholder: string;
  busy: boolean;
  modelId: string;
  reasoning: Reasoning;
  onSend: (text: string, attachments: Attachment[]) => void | Promise<void>;
  onModelChange: (modelId: string) => void;
  onReasoningChange: (reasoning: Reasoning) => void;
  onCancel?: (action: 'pause' | 'delete') => void;
}) {
  const [input, setInput] = useState('');
  const [files, setFiles] = useState<Attachment[]>([]);
  const [dragging, setDragging] = useState(false);
  const [notice, setNotice] = useState<string | undefined>(undefined);
  const ref = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  // Auto-grow to the content, capped so a long message scrolls instead.
  const onChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    e.target.style.height = 'auto';
    e.target.style.height = `${Math.min(e.target.scrollHeight, 160)}px`;
  };

  // Anything that couldn't be read is named in the notice rather than silently
  // ignored — a drop that does nothing is the worst outcome.
  const addFiles = async (list: File[]) => {
    if (list.length === 0) return;
    const room = MAX_FILES - files.length;
    if (room <= 0) { setNotice(`up to ${MAX_FILES} files per message`); return; }
    const considered = list.slice(0, room);
    const read = (await Promise.all(considered.map(readAttachment))).filter((a): a is Attachment => a !== null);
    const skipped = considered.length - read.length;
    if (read.length > 0) setFiles((prev) => [...prev, ...read]);
    setNotice(skipped > 0 ? `${skipped} file${skipped === 1 ? '' : 's'} skipped — over ${MAX_BYTES / 1024 / 1024} MB` : undefined);
  };

  const submit = async () => {
    const text = input.trim();
    if (busy || (!text && files.length === 0)) return;
    const sending = files;
    setInput(''); setFiles([]); setNotice(undefined);
    if (ref.current) ref.current.style.height = 'auto';
    await onSend(text, sending);
  };

  const canSend = !busy && (input.trim().length > 0 || files.length > 0);

  return (
    <div
      className="absolute right-0 bottom-0 left-0 z-30 p-3 pt-6"
      onDragOver={(e) => { e.preventDefault(); if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'; setDragging(true); }}
      onDragLeave={() => setDragging(false)}
      onDrop={(e) => {
        e.preventDefault();
        setDragging(false);
        const dropped = Array.from(e.dataTransfer?.files ?? []);
        if (dropped.length) void addFiles(dropped);
      }}
    >
      <div className="relative mx-auto w-full max-w-[720px]">
        {/* Confirmation sits where the user is looking — above what they're typing. */}
        <div className="absolute right-0 bottom-[calc(100%+8px)] left-0 z-30">
          <ConfirmDialog />
        </div>

        <div className={`flex flex-col rounded-lg border bg-surface transition-colors ${dragging ? 'border-dashed border-foreground/60' : 'border-border focus-within:border-foreground/40'}`}>
          <input
            ref={fileRef}
            type="file"
            multiple
            className="hidden"
            onChange={(e) => { const chosen = Array.from(e.target.files ?? []); e.target.value = ''; void addFiles(chosen); }}
          />

          {files.length > 0 && (
            <div className="flex flex-wrap gap-1.5 px-3 pt-2.5">
              {files.map((f, i) => (
                <span key={`${f.name}-${i}`} className="flex items-center gap-1.5 rounded border border-border bg-background py-1 pr-1 pl-1 font-mono text-[10px] text-muted">
                  {f.mime.startsWith('image/')
                    ? <img src={`data:${f.mime};base64,${f.data}`} alt="" className="h-5 w-5 shrink-0 rounded-sm object-cover" />
                    : <FileText size={12} className="shrink-0" />}
                  <span className="max-w-[150px] truncate">{f.name}</span>
                  <button
                    type="button"
                    aria-label={`remove ${f.name}`}
                    className="cursor-pointer border-0 bg-transparent p-0 text-muted hover:text-foreground"
                    onClick={() => setFiles((prev) => prev.filter((_, j) => j !== i))}
                  >
                    <X size={11} />
                  </button>
                </span>
              ))}
            </div>
          )}

          <textarea
            ref={ref}
            rows={1}
            value={input}
            onChange={onChange}
            onPaste={(e) => {
              // Pasted images (a screenshot straight from the clipboard) arrive as
              // files, not text; take them here instead of dumping a path.
              const pasted = Array.from(e.clipboardData?.items ?? [])
                .filter((i) => i.kind === 'file')
                .map((i) => i.getAsFile())
                .filter((f): f is File => f !== null);
              if (pasted.length) { e.preventDefault(); void addFiles(pasted); }
            }}
            onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void submit(); } }}
            placeholder={placeholder}
            className="max-h-[160px] min-h-[38px] w-full resize-none border-0 bg-transparent px-3.5 pt-2.5 pb-1 font-mono text-xs leading-relaxed text-foreground outline-none placeholder:text-muted/60"
          />
          <div className="flex items-center justify-between gap-2 px-2.5 pt-1 pb-2.5">
            <div className="flex min-w-0 items-center gap-1.5">
              <IconButton
                label="attach files"
                className="h-[30px] w-[30px] shrink-0"
                disabled={busy || files.length >= MAX_FILES}
                onClick={() => fileRef.current?.click()}
              >
                <Paperclip size={13} />
              </IconButton>
              <span className="truncate font-mono text-[10px] text-muted">{notice ?? (dragging ? 'drop to attach' : '')}</span>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <ComposerSettings modelId={modelId} reasoning={reasoning} onModelChange={onModelChange} onReasoningChange={onReasoningChange} />
              {busy && onCancel ? (
                <ContextMenu
                  title="cancel request"
                  trigger={<CircleStop size={14} />}
                  triggerClassName="control icon-btn h-[30px] w-[30px]"
                  items={[
                    { key: 'pause', label: 'pause', icon: <Pause size={13} />, onSelect: () => onCancel?.('pause') },
                    { key: 'delete', label: 'delete', icon: <Trash2 size={13} />, danger: true, dividerBefore: true, onSelect: () => onCancel?.('delete') },
                  ]}
                />
              ) : (
                <IconButton label="send" className="h-[30px] w-[30px] shrink-0" disabled={!canSend} onClick={submit}>
                  <Send size={13} />
                </IconButton>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
