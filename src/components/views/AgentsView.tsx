import { useState } from 'react';
import { Bot, Plus, Trash2 } from 'lucide-react';
import type { Agent, Tool } from '../../types';
import { AgentAvatar } from '../ui/AgentAvatar';
import { DeleteConfirm } from '../ui/DeleteConfirm';

export function AgentsView({ agents, tools, onOpen, onCreate, onDelete }: {
  agents: Agent[]; tools: Tool[]; onOpen: (id: string) => void; onCreate: () => void; onDelete: (id: string) => void;
}) {
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const isReady = (a: Agent) => !!a.name.trim() && a.name.trim() !== 'New Agent' && !!a.model.trim() && !!a.objective.trim();
  const visible = agents.filter((a) => !a.isManager);
  const [confirmTarget, setConfirmTarget] = useState<Agent | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  const openConfirm = (a: Agent) => setConfirmTarget(a);
  const closeConfirm = () => setConfirmTarget(null);

  return (
    <>
      <header className="mb-6 flex items-end justify-between">
        <div>
          <span className="font-mono text-[11px] tracking-[1px] text-muted">WORKSPACE</span>
          <h1 style={{ margin: 0, fontSize: 24 }}>Agents</h1>
        </div>
        <button className="primary" onClick={onCreate}><Plus size={14} />New agent</button>
      </header>

      {visible.length === 0 ? (
        <div className="flex min-h-[370px] flex-col items-center justify-center rounded-[16px] border border-dashed border-soft text-center text-muted">
          <Bot size={28} className="mb-3 opacity-60" />
          <h2 className="mt-[13px] mb-[7px] text-text">No agents yet</h2>
          <p className="mb-5 max-w-[360px] text-[12px] leading-[1.6]">Create your first agent — give it a name, a model, and an objective, then start chatting.</p>
          <button className="primary" onClick={onCreate}><Plus size={14} />New agent</button>
        </div>
      ) : (
        <div className="grid max-w-[1100px] grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3.5">
          {visible.map((a) => (
            <div key={a.id} className="group relative min-h-[220px] cursor-pointer rounded-[16px] border border-line bg-panel p-6 text-left transition-all duration-150 hover:-translate-y-0.5 hover:border-dotted hover:border-mid hover:shadow-lift" onClick={() => onOpen(a.id)} onMouseEnter={() => setHoveredId(a.id)} onMouseLeave={() => setHoveredId((h) => (h === a.id ? null : h))}>
              <button
                className="absolute top-3 right-3 grid h-7 w-7 cursor-pointer place-items-center rounded-md border-0 bg-transparent text-muted opacity-0 transition-opacity duration-150 group-hover:opacity-100 hover:bg-[#e11d48] hover:text-white"
                title="Delete agent"
                onClick={(e) => { e.stopPropagation(); openConfirm(a); }}
              >
                <Trash2 size={13} />
              </button>
              <div className="flex flex-col items-center">
                <AgentAvatar agent={a} size={88} playing={hoveredId === a.id} />
                <b className="mt-4 block text-[15px]">{a.name}</b>
                <small className="mt-2.5 block min-h-[36px] text-center text-[11px] leading-[1.6] text-muted">{a.objective || 'Not configured yet'}</small>
                <em className="mt-4 block font-mono text-[10px] text-muted not-italic">
                  {isReady(a) ? a.toolIds.map(toolName).join(' · ') || 'no tools' : '⚙ needs setup'}
                </em>
              </div>
            </div>
          ))}
        </div>
      )}

      {confirmTarget && (
        <DeleteConfirm
          name={confirmTarget.name}
          description="This permanently removes the agent and its configuration. Type the agent's name to confirm."
          onCancel={closeConfirm}
          onConfirm={() => { onDelete(confirmTarget.id); closeConfirm(); }}
        />
      )}
    </>
  );
}
