import { useState } from 'react';
import { FileText, ThumbsDown, ThumbsUp } from 'lucide-react';
import { Markdown } from '../ui/Markdown';
import { StreamIndicator } from '../ui/StreamIndicator';
import { IconButton } from '../ui/Button';
import type { Attachment, ChatStep } from '../../types';

// One message shape for both chat surfaces. Your own turns take the surface
// fill; the agent's prose sits on the content plane behind a hairline, so the
// two read apart without a second colour. An assistant reply can carry a
// good/bad signal that feeds the agent's memory.
const fmtMs = (ms: number) => (ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`);

// A turn that produced no text must never read as a blank bubble. Shared with the
// send paths so the stored entry isn't empty either.
export const EMPTY_REPLY = '(empty reply)';

export function MessageBubble({ role, content, attachments, streaming, steps, meta, time, durationMs, ttftMs, onRate }: {
  role: 'user' | 'assistant';
  content: string;
  attachments?: Attachment[];
  streaming?: boolean;
  steps?: ChatStep[];
  meta?: string;
  time?: string;
  durationMs?: number;
  ttftMs?: number;
  onRate?: (signal: 'up' | 'down') => void;
}) {
  const mine = role === 'user';
  const [rated, setRated] = useState<'up' | 'down' | null>(null);
  // Send time, and for a reply how long it took / how long to the first token.
  const timing = [time, durationMs != null ? fmtMs(durationMs) : '', ttftMs != null ? `${fmtMs(ttftMs)} first token` : '']
    .filter(Boolean).join(' · ');

  const rate = (signal: 'up' | 'down') => {
    if (rated) return;
    setRated(signal);
    onRate?.(signal);
  };

  return (
    <div className={`group mb-3 flex last:mb-0 ${mine ? 'justify-end' : 'justify-start'}`}>
      <div className={`flex min-w-0 max-w-[78%] flex-col ${mine ? 'items-end' : 'items-start'}`}>
        <div
          className={`max-w-full min-w-0 rounded-xl border border-border px-3.5 py-2.5 text-[13.5px] leading-[1.65] break-words ${
            mine ? 'bg-surface text-foreground whitespace-pre-wrap' : 'text-foreground'
          }`}
        >
          {meta && <span className="mb-1 block font-mono text-[10px] text-muted">{meta}</span>}
          {/* What was attached to this turn, shown as it was sent — the same
              picture back, not a filename. */}
          {attachments && attachments.length > 0 && (
            <div className={`mb-1.5 flex flex-wrap gap-1.5 ${mine ? 'justify-end' : ''}`}>
              {attachments.map((a, i) => (a.mime.startsWith('image/') ? (
                <img key={i} src={`data:${a.mime};base64,${a.data}`} alt={a.name} className="max-h-44 max-w-[240px] rounded-lg border border-border object-cover" />
              ) : (
                <span key={i} className="flex items-center gap-1.5 rounded border border-border px-2 py-1 font-mono text-[10px] text-muted">
                  <FileText size={11} className="shrink-0" />{a.name}
                </span>
              )))}
            </div>
          )}
          {mine
            ? content
            : content.trim() || streaming
              ? <Markdown>{content}</Markdown>
              : <span className="text-muted">{EMPTY_REPLY}</span>}
          {streaming && (content
            ? <span className="ml-0.5 inline-block h-3.5 w-[2px] animate-pulse bg-foreground/50 align-middle" />
            : <StreamIndicator streaming steps={steps} />)}
        </div>

        {!streaming && timing && (
          <span className={`mt-1 px-1 font-mono text-[9px] text-muted/70 ${mine ? 'text-right' : ''}`}>{timing}</span>
        )}

        {!mine && onRate && !streaming && content && (
          <div className={`mt-1 flex items-center gap-0.5 transition-opacity ${rated ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'}`}>
            <IconButton label="good reply" className="h-6 w-6" disabled={rated !== null} onClick={() => rate('up')}>
              <ThumbsUp size={11} className={rated === 'up' ? 'text-foreground' : ''} />
            </IconButton>
            <IconButton label="bad reply" className="h-6 w-6" disabled={rated !== null} onClick={() => rate('down')}>
              <ThumbsDown size={11} className={rated === 'down' ? 'text-foreground' : ''} />
            </IconButton>
          </div>
        )}
      </div>
    </div>
  );
}
