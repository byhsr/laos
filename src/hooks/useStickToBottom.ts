import { useEffect, useLayoutEffect, type RefObject } from 'react';

// Keeps a chat log pinned to its newest message.
//
// Two triggers, because one isn't enough:
//   - a message changed (or the log just mounted) → jump to the bottom
//   - the scroller gained a box → a log that mounted inside a hidden tab or view
//     is 0px tall, so its initial scroll went nowhere; pin it the moment it is
//     actually shown.
//
// Scrolling is instant: a smooth animation can be interrupted before it lands,
// which is exactly how "start at the bottom" silently stops being true.
export function useStickToBottom(ref: RefObject<HTMLElement | null>, signal: unknown) {
  useLayoutEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [ref, signal]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const pin = () => { el.scrollTop = el.scrollHeight; };
    pin();
    // Only the scroller's own box, never its content: growing text must not drag
    // a reader who has scrolled up back down.
    const ro = new ResizeObserver(pin);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
}
