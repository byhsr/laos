// Streaming status for chat. While the reply is being prepared the bubble shows
// the live step ("thinking…", "calling notion_search…") rather than a generic
// typing placeholder; once tokens flow the caller swaps in a blinking caret.
export function StreamIndicator({ streaming, status }: { streaming: boolean; status?: string }) {
  if (!streaming) return null;
  if (status) {
    return (
      <span className="inline-flex items-center gap-2 font-mono text-[11px] text-muted">
        <span className="relative flex h-1.5 w-1.5 shrink-0">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-foreground/40 opacity-60" />
          <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-foreground/50" />
        </span>
        {status}
      </span>
    );
  }
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
