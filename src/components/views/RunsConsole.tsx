import { useState } from 'react';
import { ChevronRight } from 'lucide-react';
import type { Agent, Run } from '../../types';

const fmtTime = (iso?: string) => {
  if (!iso) return '';
  const d = new Date(iso);
  return isNaN(d.getTime()) ? '' : d.toLocaleTimeString();
};

const fmtDuration = (r: Run) => {
  if (!r.startedAt) return '';
  const start = new Date(r.startedAt).getTime();
  if (isNaN(start)) return '';
  const end = r.endedAt ? new Date(r.endedAt).getTime() : Date.now();
  if (isNaN(end)) return '';
  const ms = Math.max(0, end - start);
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
};

const statusColor = (s: string) => s === 'running' ? 'text-[#facc15]' : s === 'completed' ? 'text-[#22c55e]' : 'text-[#f87171]';
const statusIcon = (s: string) => s === 'running' ? '▸' : s === 'completed' ? '✓' : '✕';

export function RunsConsole({ runs, agents, onOpenAgent, onClear }: {
  runs: Run[]; agents: Agent[]; onOpenAgent: (id: string) => void; onClear: () => void;
}) {
  const agentName = (id: string) => agents.find((a) => a.id === id)?.name ?? id;
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const toggle = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        <div><span className="font-mono text-[10px] tracking-[1px] text-muted">ACTIVITY</span><h1 style={{ margin: 0, fontSize: 24 }}>Runs</h1></div>
        <button className="secondary" onClick={onClear} disabled={runs.length === 0}>Clear log</button>
      </header>

      <div className="runs-console max-h-[calc(100vh-220px)] overflow-y-auto rounded-[10px] border border-line bg-[#0a0a0c] p-3.5 font-mono text-[12px] leading-[1.6]">
        {runs.length === 0 && (
          <div className="console-empty p-2.5 text-center text-[12px] text-muted">
            <p>No runs yet. Open an agent and send a task — every step shows up here like a live command line.</p>
          </div>
        )}
        {[...runs].sort((a, b) => new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()).map((r) => {
          const isOpen = expanded.has(r.id);
          const totalTokens = (r.promptTokens ?? 0) + (r.completionTokens ?? 0);
          return (
            <div key={r.id} className="border-b border-[#1c1c1f] last:border-0">
              {/* Collapsed log row */}
              <button className="flex w-full cursor-pointer items-center gap-2.5 border-0 bg-transparent px-1 py-2.5 text-left" onClick={() => toggle(r.id)}>
                <ChevronRight size={11} className={`shrink-0 text-mid transition-transform duration-150 ${isOpen ? 'rotate-90' : ''}`} />
                <span className={`shrink-0 text-muted ${statusColor(r.status)}`}>{statusIcon(r.status)}</span>
                <span className="shrink-0 text-mid">{fmtTime(r.startedAt)}</span>
                <button className="shrink-0 cursor-pointer border-0 bg-none p-0 font-mono text-[11px] text-text hover:underline" onClick={(e) => { e.stopPropagation(); onOpenAgent(r.agentId); }}>{agentName(r.agentId)}</button>
                <span className="shrink-0 text-muted">{r.model}</span>
                <span className="shrink-0 text-mid">{fmtDuration(r)}</span>
                {totalTokens > 0 && <span className="shrink-0 text-mid">{totalTokens.toLocaleString()} tok</span>}
                <span className={`ml-auto shrink-0 text-[10px] ${statusColor(r.status)}`}>{r.status}</span>
              </button>

              {/* Expanded details — hidden until the user clicks the row */}
              {isOpen && (
                <div className="px-1 pb-3">
                  <div className="my-1 text-[#d4d4d8]">$ {r.input}</div>
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
              )}
            </div>
          );
        })}
      </div>
    </>
  );
}
