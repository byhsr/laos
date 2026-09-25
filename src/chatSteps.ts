import type { ChatStep } from './types';
import type { StreamStep } from './runtime';

// Folds a live step event into the turn's trace. Shared by the agent window and
// the Manager so both chat surfaces report activity the same way.
export const mergeStep = (prev: ChatStep[], ev: StreamStep): ChatStep[] => {
  const last = prev[prev.length - 1];
  const time = new Date().toLocaleTimeString();

  // The model's reasoning arrives token by token, so it grows one step rather
  // than adding a line per token.
  if (ev.append) {
    if (last && last.kind === 'think' && last.streaming) {
      return [...prev.slice(0, -1), { ...last, text: last.text + ev.text }];
    }
    // The first real chunk replaces the generic "Thinking…" phase.
    const base = last && last.kind === 'think' && !last.streaming ? prev.slice(0, -1) : prev;
    return [...base, { kind: 'think', text: ev.text, time, streaming: true }];
  }

  // Each tool round re-announces its phase — collapse the repeats.
  if (last && last.kind === ev.kind && last.text === ev.text) return prev;
  return [...prev, { kind: ev.kind as ChatStep['kind'], text: ev.text, time }];
};
