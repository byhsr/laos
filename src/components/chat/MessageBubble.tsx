import { useState } from 'react';
import { ThumbsDown, ThumbsUp } from 'lucide-react';
import { Markdown } from '../ui/Markdown';
import { StreamIndicator } from '../ui/StreamIndicator';
import { IconButton } from '../ui/Button';
import type { ChatStep } from '../../types';

// One message shape for both chat surfaces. Your own turns take the surface
// fill; the agent's prose sits on the content plane behind a hairline, so the
// two read apart without a second colour. An assistant reply can carry a
// good/bad signal that feeds the agent's memory.
export function MessageBubble({ role, content, streaming, steps, meta, onRate }: {
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
  steps?: ChatStep[];
  meta?: string;
  onRate?: (signal: 'up' | 'down') => void;
}) {
  const mine = role === 'user';
  const [rated, setRated] = useState<'up' | 'down' | null>(null);

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
          {mine ? content : <Markdown>{content}</Markdown>}
          {streaming && (content
            ? <span className="ml-0.5 inline-block h-3.5 w-[2px] animate-pulse bg-foreground/50 align-middle" />
            : <StreamIndicator streaming steps={steps} />)}
        </div>

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
