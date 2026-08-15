import { Bot, Plus, Trash2 } from 'lucide-react';
import type { Agent, Tool } from '../../types';

export function AgentsView({ agents, tools, onOpen, onCreate, onDelete }: {
  agents: Agent[]; tools: Tool[]; onOpen: (id: string) => void; onCreate: () => void; onDelete: (id: string) => void;
}) {
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const isReady = (a: Agent) => !!a.name.trim() && a.name.trim() !== 'New Agent' && !!a.model.trim() && !!a.objective.trim();
  return (
    <>
      <header className="mb-6 flex items-end justify-between">
        <div>
          <span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span>
          <h1 style={{ margin: 0, fontSize: 24 }}>Agents</h1>
        </div>
        <button className="primary" onClick={onCreate}><Plus size={14} />New agent</button>
      </header>

      {agents.length === 0 ? (
        <div className="flex min-h-[370px] flex-col items-center justify-center rounded-[12px] border border-dashed border-soft text-center text-muted">
          <Bot size={28} className="mb-3 opacity-60" />
          <h2 className="mt-[13px] mb-[7px] text-text">No agents yet</h2>
          <p className="mb-5 max-w-[360px] text-[12px] leading-[1.6]">Create your first agent — give it a name, a model, and an objective, then start chatting.</p>
          <button className="primary" onClick={onCreate}><Plus size={14} />New agent</button>
        </div>
      ) : (
        <div className="grid max-w-[1100px] grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3.5">
          {agents.map((a) => (
            <div key={a.id} className="group relative min-h-[165px] cursor-pointer rounded-[10px] border border-line bg-panel p-[18px] text-left transition-transform duration-150 hover:-translate-y-0.5 hover:border-dim" onClick={() => onOpen(a.id)} style={{ borderTop: `2px solid ${a.color}` }}>
              <button
                className="absolute top-2.5 right-2.5 grid h-7 w-7 cursor-pointer place-items-center rounded-md border-0 bg-transparent text-muted opacity-0 transition-opacity duration-150 group-hover:opacity-100 hover:bg-[#e11d48] hover:text-white"
                title="Delete agent"
                onClick={(e) => { e.stopPropagation(); onDelete(a.id); }}
              >
                <Trash2 size={13} />
              </button>
              <span className="mr-1.5 inline-block h-2 w-2 rounded-full" style={{ background: a.color }} />
              <b className="mt-2.5 block text-[14px]">{a.name}</b>
              <small className="mt-1.5 block min-h-[34px] text-[11px] leading-[1.5] text-muted">{a.objective || 'Not configured yet'}</small>
              <em className="mt-3.5 block font-mono text-[9px] text-muted not-italic">
                {isReady(a) ? a.toolIds.map(toolName).join(' · ') || 'no tools' : '⚙ needs setup'}
              </em>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
