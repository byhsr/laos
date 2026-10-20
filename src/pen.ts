// The pen palette. Only token names live here — the colours themselves are
// theme tokens, so the palette is defined once per theme and never duplicated.
export const PEN_COLORS: { key: string; token: string; label: string }[] = [
  { key: 'blue', token: '--pen-blue', label: 'blue' },
  { key: 'ink', token: '--pen-ink', label: 'ink' },
  { key: 'coral', token: '--pen-coral', label: 'coral' },
  { key: 'green', token: '--pen-green', label: 'green' },
  { key: 'amber', token: '--pen-amber', label: 'amber' },
  { key: 'teal', token: '--pen-teal', label: 'teal' },
];

export const DEFAULT_PEN = 'blue';

// Resolve a pen to its theme colour, falling back through the theme's other ink
// tokens so a missing token can never leave a stroke invisible.
export const penColor = (key: string): string => {
  const cs = getComputedStyle(document.documentElement);
  const entry = PEN_COLORS.find((c) => c.key === key) ?? PEN_COLORS[0];
  return (
    cs.getPropertyValue(entry.token).trim() ||
    cs.getPropertyValue('--accent-soft').trim() ||
    cs.getPropertyValue('--ink').trim() ||
    '#2b2b33'
  );
};
