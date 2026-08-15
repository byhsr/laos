import { useMemo } from 'react';
import { Play, Plus } from 'lucide-react';
import type { Agent, Edge, Tool } from '../../types';

export function CanvasView({ agents, edges, tools, selectedAgentId, onSelect, onOpen, onCreate }: {
  agents: Agent[]; edges: Edge[]; tools: Tool[]; selectedAgentId: string | null; onSelect: (id: string) => void; onOpen: () => void; onCreate: () => void;
}) {
  const byId = useMemo(() => new Map(agents.map((a) => [a.id, a])), [agents]);
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        <div><span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span><h1 style={{ margin: 0, fontSize: 24 }}>Canvas</h1></div>
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="secondary" onClick={onCreate}><Plus size={13} />New agent</button>
          <button className="primary" onClick={onOpen} disabled={!selectedAgentId}><Play size={14} />Run selected</button>
        </div>
      </header>
      <div className="canvas relative w-full min-h-[650px] overflow-hidden rounded-[12px] border border-[#27272a] bg-[#0b0b0d] shadow-[inset_0_1px_0_#ffffff08]" style={{ backgroundImage: 'radial-gradient(#2b2b30 1px, transparent 1px)', backgroundSize: '24px 24px' }}>
        <div className="canvas-toolbar absolute top-5 right-[22px] left-[22px] z-[3] flex items-center justify-between text-[11px] text-muted">
          <span><i className="mr-1.5 inline-block h-[7px] w-[7px] rounded-full bg-[var(--green)]" />local</span>
          <button className="flex cursor-pointer items-center gap-[5px] rounded-[5px] border border-[#3f3f46] bg-[#18181b] px-2.5 py-[7px] text-muted" onClick={onOpen} disabled={!selectedAgentId}><Play size={13} />Run</button>
        </div>
        <svg className="links pointer-events-none absolute z-0 h-full w-full" width="100%" height="100%">
          {edges.map((e) => {
            const from = byId.get(e.from); const to = byId.get(e.to);
            if (!from || !to) return null;
            return <path key={e.id} className="fill-none stroke-[#71717a] stroke-[1.2] opacity-75 [stroke-dasharray:5_5]" d={`M ${from.x + 95} ${from.y + 63} C ${from.x + 170} ${from.y + 63}, ${to.x - 80} ${to.y + 63}, ${to.x + 8} ${to.y + 63}`} />;
          })}
        </svg>
        {agents.map((a) => (
          <button key={a.id} className={`agent-card absolute z-[2] w-[212px] min-h-[136px] cursor-pointer rounded-[10px] border border-[#3f3f46] bg-[#141416] p-[15px] text-left shadow-[0_12px_30px_#0007] transition-transform duration-150 hover:-translate-y-[3px] hover:border-[#71717a] hover:shadow-[0_18px_36px_#0009] active:scale-[0.98] ${a.id === selectedAgentId ? 'outline-2 outline-offset-2 outline-[var(--accent)]' : ''}`} style={{ left: a.x, top: a.y, borderTop: `2px solid ${a.color}` }} onClick={() => onSelect(a.id)}>
            <span className="agent-dot mr-1 inline-block h-2 w-2 rounded-full" style={{ background: a.color }} />
            <b className="mt-1.5 block text-[13px]">{a.name}</b>
            <small className="font-mono text-[9px] text-muted">{a.model}</small>
            <div>{a.toolIds.map((t) => <em key={t} className="mr-[3px] mt-3 inline-block rounded bg-[#27272a] px-[5px] py-1 font-mono text-[9px] text-[#d4d4d8] not-italic">{toolName(t)}</em>)}</div>
          </button>
        ))}
        <div className="canvas-note absolute bottom-5 left-[22px] z-[3] rounded-[5px] border border-[#3f3f46] bg-[#18181bd9] px-2.5 py-1.5 text-[10px] text-muted">Select an agent to open its run view.</div>
      </div>
    </>
  );
}
