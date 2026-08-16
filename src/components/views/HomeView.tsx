import { Plus, Workflow as WorkflowIcon } from 'lucide-react';
import { useState } from 'react';
import type { Agent, Run, Tool, Workflow } from '../../types';
import { AgentAvatar } from '../ui/AgentAvatar';

const SHOW = 4;

export function HomeView({ agents, tools, runs, workflows, onOpen, onCreate, onOpenWorkflow }: {
  agents: Agent[]; tools: Tool[]; runs: Run[]; workflows: Workflow[];
  onOpen: (id: string) => void; onCreate: () => void; onOpenWorkflow: (id: string) => void;
}) {
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const totalTokens = (runs ?? []).reduce((sum, r) => sum + (r.promptTokens ?? 0) + (r.completionTokens ?? 0), 0);
  const agentTokens = (id: string) => (runs ?? []).filter((r) => r.agentId === id).reduce((sum, r) => sum + (r.promptTokens ?? 0) + (r.completionTokens ?? 0), 0);
  const fmt = (n: number) => n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
  const visibleAgents = agents.filter((a) => !a.isManager);
  const [hoveredAgentId, setHoveredAgentId] = useState<string | null>(null);
  return (
    <>
      <header className="mb-6 flex items-end justify-between">
        <div>
          <span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span>
          <h1 style={{ margin: 0, fontSize: 24 }}>Home</h1>
        </div>
        <button className="primary" onClick={onCreate}><Plus size={14} />New agent</button>
      </header>
      <div className="mb-[22px] -mt-2 flex flex-wrap rounded-[10px] border border-line bg-panel">
        <div className="min-w-[140px] border-r border-line px-[22px] py-4">
          <span className="block font-mono text-[9px] tracking-[0.8px] text-muted">AGENTS</span>
          <b className="mt-[3px] block text-[20px] tracking-[-0.7px]">{visibleAgents.length}</b>
        </div>
        <div className="min-w-[140px] border-r border-line px-[22px] py-4">
          <span className="block font-mono text-[9px] tracking-[0.8px] text-muted">RUNS</span>
          <b className="mt-[3px] block text-[20px] tracking-[-0.7px]">{runs?.length ?? 0}</b>
        </div>
        <div className="min-w-[140px] border-r border-line px-[22px] py-4">
          <span className="block font-mono text-[9px] tracking-[0.8px] text-muted">TOKENS</span>
          <b className="mt-[3px] block text-[20px] tracking-[-0.7px]">{fmt(totalTokens)}</b>
        </div>
        <div className="ml-auto flex min-w-auto items-center border-0 px-[22px] py-4 text-[11px] text-[#d4d4d8]">
          <i className="mr-[7px] inline-block h-[7px] w-[7px] rounded-full bg-[var(--green)]" />Local
        </div>
      </div>

      <div className="mb-3 flex items-center justify-between">
        <span className="font-mono text-[10px] tracking-[1px] text-muted">Agents</span>
        <button className="cursor-pointer border-0 bg-none p-0 font-mono text-[10px] tracking-[1px] text-muted hover:text-text" onClick={onCreate}>+ New</button>
      </div>
      <div className="grid max-w-[1100px] grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
        {visibleAgents.slice(0, SHOW).map((a) => (
          <button key={a.id} className="home-agent flex min-h-[72px] cursor-pointer items-center gap-3 rounded-[10px] border border-line bg-panel p-4 text-left transition-all duration-150 hover:-translate-y-0.5 hover:border-dotted hover:border-mid hover:shadow-[0_8px_24px_#0005]" onClick={() => onOpen(a.id)} onMouseEnter={() => setHoveredAgentId(a.id)} onMouseLeave={() => setHoveredAgentId((h) => (h === a.id ? null : h))}>
            <AgentAvatar agent={a} size={40} playing={hoveredAgentId === a.id} />
            <div className="min-w-0">
              <b className="block truncate text-[13px]">{a.name}</b>
              <em className="block truncate font-mono text-[9px] text-muted not-italic">{a.toolIds.map(toolName).join(' · ') || 'no tools'} · {fmt(agentTokens(a.id))} tok</em>
            </div>
          </button>
        ))}
      </div>

      <div className="mt-6 mb-3 flex items-center justify-between">
        <span className="font-mono text-[10px] tracking-[1px] text-muted">Workflows</span>
        <button className="cursor-pointer border-0 bg-none p-0 font-mono text-[10px] tracking-[1px] text-muted hover:text-text" onClick={() => onOpenWorkflow('new')}>+ New</button>
      </div>
      <div className="grid max-w-[1100px] grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
        {workflows.slice(0, SHOW).map((w) => (
          <button key={w.id} className="home-agent flex min-h-[72px] cursor-pointer items-center gap-3 rounded-[10px] border border-line bg-panel p-4 text-left transition-all duration-150 hover:-translate-y-0.5 hover:border-dotted hover:border-mid hover:shadow-[0_8px_24px_#0005]" onClick={() => onOpenWorkflow(w.id)}>
            <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-panel2 text-muted"><WorkflowIcon size={16} /></span>
            <div className="min-w-0">
              <b className="block truncate text-[13px]">{w.name}</b>
              <em className="block truncate font-mono text-[9px] text-muted not-italic">{w.nodes.length} nodes · {w.edges.length} connections</em>
            </div>
          </button>
        ))}
      </div>
    </>
  );
}
