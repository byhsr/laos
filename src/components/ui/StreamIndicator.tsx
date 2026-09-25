import { useState } from 'react';
import { ChevronRight } from 'lucide-react';
import type { ChatStep } from '../../types';

// Streaming status for chat. Rather than a fixed "thinking…" label, this shows
// the live step the backend reported — and, when there's a trace worth reading
// (a tool round, or the model's reasoning), a dropdown that opens the whole
// timeline of what the agent has actually done this turn.
//
// While tokens are flowing the caller swaps in a blinking caret instead.
export function StreamIndicator({ streaming, steps = [] }: { streaming: boolean; steps?: ChatStep[] }) {
  const [open, setOpen] = useState(false);

  if (!streaming) return null;

  const last = steps[steps.length - 1];
  if (!last) {
    return (
      <span className="ml-1 inline-flex items-center gap-[3px] align-middle">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="inline-block h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/40"
            style={{ animationDelay: `${i * 120}ms` }}
          />
        ))}
      </span>
    );
  }

  const expandable = steps.length > 1 || !!last.streaming;

  return (
    <div className="flex min-w-0 flex-col">
      <div className="flex min-w-0 items-center gap-1.5">
        <span className="relative flex h-1.5 w-1.5 shrink-0">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-foreground/40 opacity-60" />
          <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-foreground/50" />
        </span>
        <span className="min-w-0 truncate font-mono text-[11px] text-muted">{last.text}</span>
        {expandable && (
          <button
            type="button"
            aria-label={open ? 'hide steps' : 'show steps'}
            aria-expanded={open}
            onClick={() => setOpen((o) => !o)}
            className="focus-ring grid h-4 w-4 shrink-0 cursor-pointer place-items-center rounded border-0 bg-transparent p-0 text-muted transition-colors hover:text-foreground"
          >
            <ChevronRight size={11} className={`transition-transform duration-150 ${open ? 'rotate-90' : ''}`} />
          </button>
        )}
      </div>

      {open && (
        <ol className="mt-2 mb-0 flex max-h-[240px] list-none flex-col gap-1.5 overflow-x-hidden overflow-y-auto border-l border-border pl-2.5">
          {steps.map((s, i) => (
            <li key={i} className="flex min-w-0 gap-2 font-mono text-[10px] leading-relaxed">
              <span className="shrink-0 text-muted/70">{s.time}</span>
              <span className="w-9 shrink-0 tracking-wider text-muted uppercase">{s.kind}</span>
              <span className="min-w-0 whitespace-pre-wrap break-words text-foreground/70">{s.text}</span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
