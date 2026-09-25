// One tab grammar for the whole app. Selection reads through a surface fill plus
// foreground brightness — never an outline in a second colour.
export const tabCls = (active: boolean) =>
  `focus-ring flex shrink-0 cursor-pointer items-center gap-1 rounded-lg border px-2.5 py-1 font-mono text-[11px] lowercase transition-colors ${
    active ? 'border-border bg-surface text-foreground' : 'border-transparent text-muted hover:bg-surface hover:text-foreground'
  }`;
