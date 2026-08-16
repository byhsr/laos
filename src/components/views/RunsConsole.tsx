import { useEffect, useRef } from 'react';
import type { Agent, Run } from '../../types';

export function RunsConsole({ runs, agents, onOpenAgent, onClear }: {
  runs: Run[]; agents: Agent[]; onOpenAgent: (id: string) => void; onClear: () => void;
}) {
  const agentName = (id: string) => agents.find((a) => a.id === id)?.name ?? id;
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [runs]);

  const statusColor = (s: string) => s === 'running' ? 'text-[#facc15]' : s === 'completed' ? 'text-[#22c55e]' : 'text-[#f87171]';

  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        <div><span className="font-mono text-[10px] tracking-[1px] text-muted">ACTIVITY</span><h1 style={{ margin: 0, fontSize: 24 }}>Runs</h1></div>
        <button className="secondary" onClick={onClear} disabled={runs.length === 0}>Clear log</button>
      </header>

      <div className="runs-console max-h-[calc(100vh-220px)] overflow-y-auto rounded-[10px] border border-line bg-[#0a0a0c] p-3.5 font-mono text-[12px] leading-[1.6]" ref={scrollRef}>
        {runs.length === 0 && (
          <div className="console-empty p-2.5 text-center text-[12px] text-muted">
            <p>No runs yet. Open an agent and send a task — every step shows up here like a live command line.</p>
          </div>
        )}
        {runs.map((r) => (
          <div key={r.id} className="border-b border-[#1c1c1f] py-3 last:border-0">
            <div className="console-head flex items-center gap-2.5 text-[11px]">
              <span className={`text-muted ${statusColor(r.status)}`}>{r.status === 'running' ? '▸' : r.status === 'completed' ? '✓' : '✕'}</span>
              <button className="cursor-pointer border-0 bg-none p-0 font-mono text-[11px] text-text hover:underline" onClick={() => onOpenAgent(r.agentId)}>{agentName(r.agentId)}</button>
              <span className="text-muted">{r.model}</span>
              {(r.promptTokens || r.completionTokens) ? <span className="text-mid">{((r.promptTokens ?? 0) + (r.completionTokens ?? 0)).toLocaleString()} tok</span> : null}
              <span className="ml-auto text-mid">{r.startedAt ? (() => { const d = new Date(r.startedAt); return isNaN(d.getTime()) ? '' : d.toLocaleTimeString(); })() : ''}</span>
            </div>
            <div className="my-1.5 text-[#d4d4d8]">$ {r.input}</div>
            {(r.events ?? []).map((ev, i) => (
              <div key={i} className="console-line flex items-baseline gap-2">
                <span className="flex-none text-mid">{ev.time}</span>
                <span className={`w-10 flex-none text-muted ${ev.type === 'tool' ? 'text-[#38bdf8]' : ev.type === 'thought' ? 'text-[#c4b5fd]' : 'text-[#22c55e]'}`}>{ev.type === 'tool' ? 'tool' : ev.type === 'thought' ? 'think' : 'out'}</span>
                <span className={`text-[#a1a1aa] ${ev.type === 'tool' ? 'text-[#7dd3fc]' : ev.type === 'thought' ? 'text-[#c4b5fd]' : ''}`}>{ev.title}{ev.detail ? ` — ${ev.detail}` : ''}</span>
              </div>
            ))}
            {r.output && <pre className="mt-1.5 ml-12 whitespace-pre-wrap rounded-[6px] border border-[#1c1c1f] bg-[#111113] p-2 text-[11px] text-[#e4e4e7]">{r.output}</pre>}
            {r.status === 'failed' && <div className="console-line flex items-baseline gap-2"><span className="flex-none text-mid" /><span className="w-10 flex-none text-[#22c55e]">err</span><span className="text-[#a1a1aa]">Run failed — see agent chat for details.</span></div>}
          </div>
        ))}
      </div>
    </>
  );
}
