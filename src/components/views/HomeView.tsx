import { Plus } from 'lucide-react';
import type { Agent, Run, Tool } from '../../types';

export function HomeView({ agents, tools, runs, onOpen, onCreate }: { agents: Agent[]; tools: Tool[]; runs: Run[]; onOpen: (id: string) => void; onCreate: () => void }) {
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const totalTokens = (runs ?? []).reduce((sum, r) => sum + (r.promptTokens ?? 0) + (r.completionTokens ?? 0), 0);
  const agentTokens = (id: string) => (runs ?? []).filter((r) => r.agentId === id).reduce((sum, r) => sum + (r.promptTokens ?? 0) + (r.completionTokens ?? 0), 0);
  const fmt = (n: number) => n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
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
          <b className="mt-[3px] block text-[20px] tracking-[-0.7px]">{agents.length}</b>
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
      <div className="grid max-w-[1100px] grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3.5">
        {agents.filter((a) => !a.isManager).map((a) => (
          <button key={a.id} className="home-agent min-h-[165px] cursor-pointer rounded-[10px] border border-line bg-panel p-[18px] text-left transition-transform duration-150 hover:-translate-y-0.5 hover:border-dim" onClick={() => onOpen(a.id)} style={{ borderTop: `2px solid ${a.color}` }}>
            <span className="agent-dot mr-1.5 inline-block h-2 w-2 rounded-full" style={{ background: a.color }} />
            <b className="mt-2.5 block text-[14px]">{a.name}</b>
            <small className="mt-1.5 block min-h-[34px] text-[11px] leading-[1.5] text-muted">{a.objective}</small>
            <em className="mt-3.5 block font-mono text-[9px] text-muted not-italic">{a.toolIds.map(toolName).join(' · ') || 'no tools'}</em>
            <em className="mt-1.5 block font-mono text-[9px] text-muted not-italic">{fmt(agentTokens(a.id))} tokens used</em>
          </button>
        ))}
        <div className="home-empty col-span-full flex min-h-[370px] flex-col items-center justify-center rounded-[12px] border border-dashed border-soft text-center text-muted">
          <h2 className="mt-[13px] mb-[7px] text-text">Configure your workspace</h2>
          <p className="mb-5 max-w-[360px] text-[12px] leading-[1.6]">Add agents, connect integrations, and run local-first workflows — all locally.</p>
          <button className="primary" onClick={onCreate}><Plus size={14} />New agent</button>
        </div>
      </div>
    </>
  );
}
