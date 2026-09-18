import type { Agent, Run } from '../types';

const fmt = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <span className="flex items-baseline gap-1.5">
      <span className="font-mono text-[10px] tracking-[0.6px] text-ink/60">{label}</span>
      <b className="text-[12px] font-semibold text-ink">{value}</b>
    </span>
  );
}

// The workspace's little character: a solid lime chip next to the sidebar
// toggle. Its face changes with what the workspace is doing, and it opens up on
// hover to show the vitals.
export function Vitals({ agents, runs }: { agents: Agent[]; runs: Run[] }) {
  const agentCount = agents.filter((a) => !a.isManager).length;
  const runCount = runs?.length ?? 0;
  const totalTokens = (runs ?? []).reduce((sum, r) => sum + (r.promptTokens ?? 0) + (r.completionTokens ?? 0), 0);
  const running = (runs ?? []).some((r) => r.status === 'running');

  const face = running ? '⚡' : runCount ? '🤖' : '💤';

  return (
    <div
      className="group app-no-drag relative flex h-6 w-7 shrink-0 cursor-default items-center overflow-hidden rounded-[10px] bg-lime px-1.5 shadow-soft transition-[width] duration-300 ease-out hover:w-[318px] hover:px-2.5"
      title="Workspace vitals"
    >
      <span className="grid h-4 w-4 shrink-0 place-items-center text-[13px] leading-none">{face}</span>

      <div className="ml-1.5 flex min-w-0 flex-1 items-center gap-3.5 whitespace-nowrap opacity-0 transition-opacity duration-200 group-hover:opacity-100">
        <Stat label="AGENTS" value={agentCount} />
        <Stat label="RUNS" value={runCount} />
        <Stat label="TOKENS" value={fmt(totalTokens)} />
        <span className={`ml-auto font-mono text-[10px] font-medium tracking-[0.6px] text-ink/70 ${running ? 'animate-pulse' : ''}`}>
          {running ? 'WORKING' : 'LOCAL'}
        </span>
      </div>
    </div>
  );
}
