import { Markdown } from '../ui/Markdown';
import { StreamIndicator } from '../ui/StreamIndicator';
import type { ChatStep } from '../../types';

// One message shape for both chat surfaces. Your own turns take the surface
// fill; the agent's prose sits on the content plane behind a hairline, so the
// two read apart without a second colour.
export function MessageBubble({ role, content, streaming, steps, meta }: {
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
  steps?: ChatStep[];
  meta?: string;
}) {
  const mine = role === 'user';
  return (
    <div className={`mb-3 flex last:mb-0 ${mine ? 'justify-end' : 'justify-start'}`}>
      <div
        className={`max-w-[78%] min-w-0 rounded-xl border border-border px-3.5 py-2.5 text-[13.5px] leading-[1.65] break-words ${
          mine ? 'bg-surface text-foreground whitespace-pre-wrap' : 'text-foreground'
        }`}
      >
        {meta && <span className="mb-1 block font-mono text-[10px] text-muted">{meta}</span>}
        {mine ? content : <Markdown>{content}</Markdown>}
        {streaming && (content
          ? <span className="ml-0.5 inline-block h-3.5 w-[2px] animate-pulse bg-foreground/50 align-middle" />
          : <StreamIndicator streaming steps={steps} />)}
      </div>
    </div>
  );
}
