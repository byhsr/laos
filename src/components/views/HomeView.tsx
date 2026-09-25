import { Workflow as WorkflowIcon } from 'lucide-react';
import { useState } from 'react';
import type { Agent, Integration, Skill, Tool, Workflow } from '../../types';
import { AgentAvatar } from '../ui/AgentAvatar';
import { GROUP_LABEL_CLS } from '../ui/Input';
import { GraphView } from './GraphView';

const SHOW = 4;

export type HomeTab = 'overview' | 'graph';

// No header: the section title, the Overview/Graph tabs and "new agent" all live
// in the topbar. This view opens straight into content.
export function HomeView({ agents, tools, skills, integrations, workflows, onOpen, onCreate, onOpenWorkflow, onSaveAgent, onSaveWorkflow, tab }: {
  agents: Agent[]; tools: Tool[]; skills: Skill[]; integrations: Integration[]; workflows: Workflow[];
  onOpen: (id: string) => void; onCreate: () => void; onOpenWorkflow: (id: string) => void;
  onSaveAgent: (a: Agent) => Promise<void>; onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
  tab: HomeTab;
}) {
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const visibleAgents = agents.filter((a) => !a.isManager);
  const [hoveredAgentId, setHoveredAgentId] = useState<string | null>(null);

  const newLink = 'focus-ring cursor-pointer rounded border-0 bg-transparent p-0 font-mono text-[10px] text-muted transition-colors hover:text-foreground';
  const tile = 'focus-ring flex min-h-[64px] cursor-pointer items-center gap-3 rounded-xl border border-border bg-surface p-3.5 text-left transition-colors duration-150 hover:bg-background';

  return (
    <div className="flex h-full flex-col">
      {/* Overview stays mounted so Graph's pan/zoom survives view switches. */}
      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'overview' ? '' : 'hidden'}`}>
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
      </div>

      <div className={`min-h-0 flex-1 ${tab === 'graph' ? '' : 'hidden'}`}>
        <GraphView agents={agents} skills={skills} tools={tools} integrations={integrations} workflows={workflows} onOpenAgent={onOpen} onOpenWorkflow={onOpenWorkflow} onSaveAgent={onSaveAgent} onSaveWorkflow={onSaveWorkflow} />
      </div>
    </div>
  );
}
