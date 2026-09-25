import { Workflow as WorkflowIcon } from 'lucide-react';
import { useMemo, useState } from 'react';
import type { Agent, Run, Tool, Workflow } from '../../types';
import { AgentAvatar } from '../ui/AgentAvatar';
import { GROUP_LABEL_CLS } from '../ui/Input';
import { Select, type SelectOption } from '../ui/Select';

const SHOW = 4;

const fmt = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));

// Safe short day label — DB timestamps can be empty or malformed.
const dayLabel = (s: string) => {
  const d = new Date(s);
  return isNaN(d.getTime()) ? 'unknown' : d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
};

// The readout grid: quiet boxes, one number each, with the token count given the
// dominant cell and a faded caption pinned to the bottom of every box.
const cell = 'flex min-h-0 flex-col justify-between overflow-hidden rounded-xl border border-border bg-surface p-5';

// How the token total is broken down. `total` is the default and a real entry in
// the list, so the plain total is always one selection away.
const TOKEN_FILTERS: SelectOption[] = [
  { value: 'total', label: 'total' },
  { value: 'agent', label: 'by agent' },
  { value: 'day', label: 'by day' },
];

// No header: the section's own name lives in the rail, and the graph is a
// section of its own. This view opens straight into content.
export function HomeView({ agents, tools, workflows, runs, onOpen, onCreate, onOpenWorkflow }: {
  agents: Agent[]; tools: Tool[]; workflows: Workflow[]; runs: Run[];
  onOpen: (id: string) => void; onCreate: () => void; onOpenWorkflow: (id: string) => void;
}) {
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const visibleAgents = agents.filter((a) => !a.isManager);
  const [hoveredAgentId, setHoveredAgentId] = useState<string | null>(null);
  const [tokenGrouping, setTokenGrouping] = useState<'agent' | 'day' | null>(null);

  const promptTokens = runs.reduce((sum, r) => sum + (r.promptTokens ?? 0), 0);
  const completionTokens = runs.reduce((sum, r) => sum + (r.completionTokens ?? 0), 0);

  // Same total either way — the filter only decides how it's broken down, and
  // nothing is broken down until you ask for one.
  const tokenGroups = useMemo(() => {
    if (!tokenGrouping) return [];
    const totals = new Map<string, number>();
    for (const r of runs) {
      const key = tokenGrouping === 'agent'
        ? (agents.find((a) => a.id === r.agentId)?.name ?? r.agentId)
        : dayLabel(r.startedAt);
      totals.set(key, (totals.get(key) ?? 0) + (r.promptTokens ?? 0) + (r.completionTokens ?? 0));
    }
    return [...totals.entries()].filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1]);
  }, [runs, agents, tokenGrouping]);

  const newLink = 'focus-ring cursor-pointer rounded border-0 bg-transparent p-0 font-mono text-[10px] text-muted transition-colors hover:text-foreground';
  const tile = 'focus-ring flex min-h-[64px] cursor-pointer items-center gap-3 rounded-xl border border-border bg-surface p-3.5 text-left transition-colors duration-150 hover:bg-background';

  return (
    <>
      <div className="mb-5 grid auto-rows-[104px] grid-cols-3 gap-3">
        <div className={`${cell} col-span-2 row-span-2`}>
          <div className="flex shrink-0 items-start justify-between gap-3">
            <span className="text-[38px] leading-none font-bold tracking-tight text-foreground">{fmt(promptTokens + completionTokens)}</span>
            <div className="w-[104px] shrink-0">
              <Select
                value={tokenGrouping ?? 'total'}
                options={TOKEN_FILTERS}
                onChange={(v) => setTokenGrouping(v === 'total' ? null : (v as 'agent' | 'day'))}
              />
            </div>
          </div>

          <div className="mt-3 flex shrink-0 items-baseline justify-between gap-3">
            <span className={GROUP_LABEL_CLS}>tokens</span>
            <span className="font-mono text-[10px] text-muted">{fmt(promptTokens)} in · {fmt(completionTokens)} out</span>
          </div>

          {tokenGrouping && (
            <div className="mt-2 min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
              {tokenGroups.length === 0
                ? <span className="font-mono text-[10px] text-muted">no runs recorded yet</span>
                : tokenGroups.map(([label, n]) => (
                  <div key={label} className="flex items-center justify-between gap-2 border-b border-border py-1.5 font-mono text-[10px] last:border-0">
                    <span className="min-w-0 truncate text-muted">{label}</span>
                    <span className="shrink-0 text-foreground">{fmt(n)}</span>
                  </div>
                ))}
            </div>
          )}
        </div>
        <div className={cell}>
          <span className="text-[22px] leading-none font-bold text-foreground">{runs.length}</span>
          <span className={GROUP_LABEL_CLS}>runs</span>
        </div>
        <div className={cell}>
          <span className="text-[22px] leading-none font-bold text-foreground">{visibleAgents.length}</span>
          <span className={GROUP_LABEL_CLS}>agents</span>
        </div>
      </div>

      <div className="mb-2 flex items-center justify-between gap-3">
        <span className={GROUP_LABEL_CLS}>agents</span>
        <button className={newLink} onClick={onCreate}>+ new</button>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-2.5">
        {visibleAgents.slice(0, SHOW).map((a) => (
          <button key={a.id} className={tile} onClick={() => onOpen(a.id)} onMouseEnter={() => setHoveredAgentId(a.id)} onMouseLeave={() => setHoveredAgentId((h) => (h === a.id ? null : h))}>
            <AgentAvatar agent={a} size={38} playing={hoveredAgentId === a.id} />
            <div className="min-w-0">
              <b className="block truncate font-mono text-[11px] text-foreground">{a.name}</b>
              <em className="block truncate font-mono text-[10px] text-muted not-italic">{a.toolIds.map(toolName).join(' · ') || 'no tools'}</em>
            </div>
          </button>
        ))}
      </div>

      <div className="mt-5 mb-2 flex items-center justify-between gap-3">
        <span className={GROUP_LABEL_CLS}>workflows</span>
        <button className={newLink} onClick={() => onOpenWorkflow('new')}>+ new</button>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-2.5">
        {workflows.slice(0, SHOW).map((w) => (
          <button key={w.id} className={tile} onClick={() => onOpenWorkflow(w.id)}>
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-muted"><WorkflowIcon size={15} /></span>
            <div className="min-w-0">
              <b className="block truncate font-mono text-[11px] text-foreground">{w.name}</b>
              <em className="block truncate font-mono text-[10px] text-muted not-italic">{w.nodes.length} nodes · {w.edges.length} connections</em>
            </div>
          </button>
        ))}
      </div>
    </>
  );
}
