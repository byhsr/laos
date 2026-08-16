// Animated "typing" indicator for streaming chat: three bouncing dots while
// waiting for the first token, then a blinking caret once text is flowing.
export function StreamIndicator({ streaming }: { streaming: boolean }) {
  if (!streaming) return null;
  return (
    <span className="ml-1 inline-flex items-center gap-[3px] align-middle">
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="inline-block h-1.5 w-1.5 animate-bounce rounded-full bg-muted"
          style={{ animationDelay: `${i * 120}ms` }}
        />
      ))}
    </span>
  );
}
