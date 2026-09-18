import { Home as HomeIcon, Plus, Workflow as WorkflowIcon } from 'lucide-react';
import { useState } from 'react';
import type { Agent, Integration, Run, Skill, Tool, Workflow } from '../../types';
import { AgentAvatar } from '../ui/AgentAvatar';
import { GraphView } from './GraphView';

const SHOW = 4;

export type HomeTab = 'overview' | 'graph';

const tabBtn = (active: boolean) =>
  `flex cursor-pointer items-center gap-1 rounded-[10px] border px-2.5 py-1.5 text-[11px] capitalize ${active ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`;

export function HomeView({ agents, tools, skills, integrations, runs, workflows, onOpen, onCreate, onOpenWorkflow, tab, onTabChange }: {
  agents: Agent[]; tools: Tool[]; skills: Skill[]; integrations: Integration[]; runs: Run[]; workflows: Workflow[];
  onOpen: (id: string) => void; onCreate: () => void; onOpenWorkflow: (id: string) => void;
  tab: HomeTab; onTabChange: (t: HomeTab) => void;
}) {
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const agentTokens = (id: string) => (runs ?? []).filter((r) => r.agentId === id).reduce((sum, r) => sum + (r.promptTokens ?? 0) + (r.completionTokens ?? 0), 0);
  const fmt = (n: number) => n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
  const visibleAgents = agents.filter((a) => !a.isManager);
  const [hoveredAgentId, setHoveredAgentId] = useState<string | null>(null);

  return (
    <div className="flex h-full flex-col">
      <header className="mb-5 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          <HomeIcon size={14} className="text-[var(--green)]" />
          <span className="font-mono text-[11px] uppercase tracking-[1px] text-text">Home</span>
        </div>
        <div className="flex items-center gap-1">
          <button className={tabBtn(tab === 'overview')} onClick={() => onTabChange('overview')}>Overview</button>
          <button className={tabBtn(tab === 'graph')} onClick={() => onTabChange('graph')}>Graph</button>
          <button className="primary ml-1" onClick={onCreate}><Plus size={13} />New agent</button>
        </div>
      </header>

      {/* Overview stays mounted so Graph's pan/zoom survives tab switches. */}
      <div className={`min-h-0 flex-1 overflow-y-auto ${tab === 'overview' ? '' : 'hidden'}`}>
        <div className="mb-3 flex items-center justify-between">
          <span className="font-mono text-[11px] tracking-[1px] text-muted">Agents</span>
          <button className="cursor-pointer border-0 bg-none p-0 font-mono text-[11px] tracking-[1px] text-muted hover:text-text" onClick={onCreate}>+ New</button>
        </div>
        <div className="grid max-w-[1100px] grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
          {visibleAgents.slice(0, SHOW).map((a) => (
            <button key={a.id} className="home-agent flex min-h-[72px] cursor-pointer items-center gap-3 rounded-[16px] border border-line bg-panel p-4 text-left transition-all duration-150 hover:-translate-y-0.5 hover:border-dotted hover:border-mid hover:shadow-lift" onClick={() => onOpen(a.id)} onMouseEnter={() => setHoveredAgentId(a.id)} onMouseLeave={() => setHoveredAgentId((h) => (h === a.id ? null : h))}>
              <AgentAvatar agent={a} size={40} playing={hoveredAgentId === a.id} />
              <div className="min-w-0">
                <b className="block truncate text-[13px]">{a.name}</b>
                <em className="block truncate font-mono text-[10px] text-muted not-italic">{a.toolIds.map(toolName).join(' · ') || 'no tools'} · {fmt(agentTokens(a.id))} tok</em>
              </div>
            </button>
          ))}
        </div>

        <div className="mt-6 mb-3 flex items-center justify-between">
          <span className="font-mono text-[11px] tracking-[1px] text-muted">Workflows</span>
          <button className="cursor-pointer border-0 bg-none p-0 font-mono text-[11px] tracking-[1px] text-muted hover:text-text" onClick={() => onOpenWorkflow('new')}>+ New</button>
        </div>
        <div className="grid max-w-[1100px] grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
          {workflows.slice(0, SHOW).map((w) => (
            <button key={w.id} className="home-agent flex min-h-[72px] cursor-pointer items-center gap-3 rounded-[16px] border border-line bg-panel p-4 text-left transition-all duration-150 hover:-translate-y-0.5 hover:border-dotted hover:border-mid hover:shadow-lift" onClick={() => onOpenWorkflow(w.id)}>
              <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-panel2 text-muted"><WorkflowIcon size={16} /></span>
              <div className="min-w-0">
                <b className="block truncate text-[13px]">{w.name}</b>
                <em className="block truncate font-mono text-[10px] text-muted not-italic">{w.nodes.length} nodes · {w.edges.length} connections</em>
              </div>
            </button>
          ))}
        </div>
      </div>

      <div className={`min-h-0 flex-1 ${tab === 'graph' ? '' : 'hidden'}`}>
        <GraphView embedded agents={agents} skills={skills} tools={tools} integrations={integrations} workflows={workflows} onOpenAgent={onOpen} onOpenWorkflow={onOpenWorkflow} />
      </div>
    </div>
  );
}
