// Compact number formatting for chrome readouts. A count that reaches a million
// steps up to M (then B) instead of reading as an absurd "1237k".

export const fmtCompact = (n: number): string => {
  const abs = Math.abs(n);
  if (abs >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(2)}B`;
  if (abs >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (abs >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
};

// The exact value, grouped — for a tooltip or a detail line where precision
// matters more than width.
export const fmtFull = (n: number): string => n.toLocaleString();
