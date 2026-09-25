import { Bot, Moon, Zap } from 'lucide-react';
import type { Agent, Run } from '../types';

const fmt = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <span className="flex items-baseline gap-1.5">
      <span className="font-mono text-[10px] tracking-wider text-muted uppercase">{label}</span>
      <b className="font-mono text-[11px] font-medium text-foreground">{value}</b>
    </span>
  );
}

// The workspace's little instrument readout, sitting beside the sidebar toggle.
// Its glyph changes with what the workspace is doing, and it opens up on hover
// to show the vitals — quiet at rest, informative on demand.
export function Vitals({ agents, runs }: { agents: Agent[]; runs: Run[] }) {
  const agentCount = agents.filter((a) => !a.isManager).length;
  const runCount = runs?.length ?? 0;
  const totalTokens = (runs ?? []).reduce((sum, r) => sum + (r.promptTokens ?? 0) + (r.completionTokens ?? 0), 0);
  const running = (runs ?? []).some((r) => r.status === 'running');

  const Face = running ? Zap : runCount ? Bot : Moon;

  return (
    <div className="group app-no-drag relative flex h-7 w-8 shrink-0 cursor-default items-center overflow-hidden rounded-lg px-1.5 transition-[width] duration-200 ease-out hover:w-[320px] hover:px-2.5">
      <Face size={13} className={`shrink-0 ${running ? 'animate-pulse text-foreground' : 'text-muted'}`} />

      <div className="ml-2 flex min-w-0 flex-1 items-center gap-3.5 whitespace-nowrap opacity-0 transition-opacity duration-150 group-hover:opacity-100">
        <Stat label="agents" value={agentCount} />
        <Stat label="runs" value={runCount} />
        <Stat label="tokens" value={fmt(totalTokens)} />
        <span className={`ml-auto font-mono text-[10px] text-muted ${running ? 'animate-pulse text-foreground' : ''}`}>
          {running ? 'working' : 'local'}
        </span>
      </div>
    </div>
  );
}
