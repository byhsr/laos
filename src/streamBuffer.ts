// Streamed replies arrive one token at a time. Applying each delta directly means
// a store write, a full conversation array copy, a markdown re-parse and a scroll
// on every single token — work that grows quadratically with the reply length.
//
// This batches deltas onto a short timer so the UI updates at a readable cadence
// instead. The visible result is identical; only the number of renders changes.

const FLUSH_MS = 70;

export type DeltaBuffer = {
  /** Queue a token; the accumulated text is flushed on the timer. */
  push: (delta: string) => void;
  /** Flush the accumulated text immediately and stop the timer. */
  end: () => void;
};

export function createDeltaBuffer(onFlush: (text: string) => void, intervalMs = FLUSH_MS): DeltaBuffer {
  let text = '';
  let timer: ReturnType<typeof setTimeout> | null = null;

  const flush = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    onFlush(text);
  };

  return {
    push: (delta) => {
      text += delta;
      if (timer === null) timer = setTimeout(flush, intervalMs);
    },
    end: flush,
  };
}
