import { Workflow as WorkflowIcon } from 'lucide-react';
import { useState } from 'react';
import type { Agent, Integration, Skill, Tool, Workflow } from '../../types';
import { AgentAvatar } from '../ui/AgentAvatar';
import { GraphView } from './GraphView';

const SHOW = 4;

export type HomeTab = 'overview' | 'graph';

// No header: the section title, the Overview/Graph tabs and "New agent" all live
// in the topbar (Graph is its own topbar button, + creates an agent).
export function HomeView({ agents, tools, skills, integrations, workflows, onOpen, onCreate, onOpenWorkflow, onSaveAgent, onSaveWorkflow, tab }: {
  agents: Agent[]; tools: Tool[]; skills: Skill[]; integrations: Integration[]; workflows: Workflow[];
  onOpen: (id: string) => void; onCreate: () => void; onOpenWorkflow: (id: string) => void;
  onSaveAgent: (a: Agent) => Promise<void>; onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
  tab: HomeTab;
}) {
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const visibleAgents = agents.filter((a) => !a.isManager);
  const [hoveredAgentId, setHoveredAgentId] = useState<string | null>(null);

  return (
    <div className="flex h-full flex-col">
      {/* Overview stays mounted so Graph's pan/zoom survives view switches. */}
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
                <em className="block truncate font-mono text-[10px] text-muted not-italic">{a.toolIds.map(toolName).join(' · ') || 'no tools'}</em>
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
        <GraphView embedded agents={agents} skills={skills} tools={tools} integrations={integrations} workflows={workflows} onOpenAgent={onOpen} onOpenWorkflow={onOpenWorkflow} onSaveAgent={onSaveAgent} onSaveWorkflow={onSaveWorkflow} />
      </div>
    </div>
  );
}
