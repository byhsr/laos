import { useEffect, useState } from 'react';
import { ChevronRight, RefreshCw } from 'lucide-react';
import type { Agent, Run } from '../../types';
import { listTelegramLogs, telegramWebhookHealth, type TelegramLogEntry, type WebhookHealth } from '../../runtime';

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

export function RunsConsole({ runs, agents, onOpenAgent, onClear, embedded = false }: {
  runs: Run[]; agents: Agent[]; onOpenAgent: (id: string) => void; onClear: () => void; embedded?: boolean;
}) {
  const agentName = (id: string) => agents.find((a) => a.id === id)?.name ?? id;
  const [tab, setTab] = useState<'logs' | 'webhooks'>('logs');
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [health, setHealth] = useState<WebhookHealth | null>(null);
  const [tLogs, setTLogs] = useState<TelegramLogEntry[]>([]);

  const loadWebhooks = async () => {
    setHealth(await telegramWebhookHealth());
    setTLogs(await listTelegramLogs());
  };
  useEffect(() => {
    loadWebhooks();
    const iv = setInterval(loadWebhooks, 4000);
    return () => clearInterval(iv);
  }, []);

  const toggle = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 16 }}>
        {!embedded && <div><span className="font-mono text-[11px] tracking-[1px] text-muted">ACTIVITY</span><h1 style={{ margin: 0, fontSize: 24 }}>Runs</h1></div>}
        {tab === 'logs' && <button className="secondary ml-auto" onClick={onClear} disabled={runs.length === 0}>Clear log</button>}
      </header>

      {/* Tab bar */}
      <div className="mb-4 flex items-center gap-1">
        <button className={`flex cursor-pointer items-center gap-1 rounded-[10px] border px-2.5 py-1.5 text-[11px] capitalize ${tab === 'logs' ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`} onClick={() => setTab('logs')}>Logs</button>
        <button className={`flex cursor-pointer items-center gap-1 rounded-[10px] border px-2.5 py-1.5 text-[11px] capitalize ${tab === 'webhooks' ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`} onClick={() => setTab('webhooks')}>
          Webhooks
          {health?.receiverListening === false && <i className="inline-block h-1.5 w-1.5 rounded-full bg-[#f87171]" />}
        </button>
      </div>

      {tab === 'logs' && (
      <div className="runs-console max-h-[calc(100vh-260px)] overflow-y-auto rounded-[16px] border border-line bg-inset p-3.5 font-mono text-[12px] leading-[1.6]">
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
              <button className="flex w-full cursor-pointer items-center gap-3 border-0 bg-transparent px-1 py-2.5 text-left" onClick={() => toggle(r.id)}>
                <ChevronRight size={11} className={`shrink-0 text-mid transition-transform duration-150 ${isOpen ? 'rotate-90' : ''}`} />
                <span className={`shrink-0 text-muted ${statusColor(r.status)}`}>{statusIcon(r.status)}</span>
                <span className="shrink-0 text-mid">{fmtTime(r.startedAt)}</span>
                <button className="shrink-0 cursor-pointer border-0 bg-none p-0 font-mono text-[11px] text-text hover:underline" onClick={(e) => { e.stopPropagation(); onOpenAgent(r.agentId); }}>{agentName(r.agentId)}</button>
                <span className="shrink-0 text-muted">{r.model}</span>
                <span className="shrink-0 text-mid">{fmtDuration(r)}</span>
                {totalTokens > 0 && <span className="shrink-0 text-mid">{totalTokens.toLocaleString()} tok</span>}
                <span className={`ml-auto shrink-0 text-[11px] ${statusColor(r.status)}`}>{r.status}</span>
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
                  {r.output && <pre className="mt-1.5 ml-12 whitespace-pre-wrap rounded-[10px] border border-[#1c1c1f] bg-[#111113] p-2 text-[11px] text-[#e4e4e7]">{r.output}</pre>}
                  {r.status === 'failed' && <div className="console-line flex items-baseline gap-2"><span className="flex-none text-mid" /><span className="w-10 flex-none text-[#22c55e]">err</span><span className="text-[#a1a1aa]">Run failed — see agent chat for details.</span></div>}
                </div>
              )}
            </div>
          );
        })}
      </div>
      )}

      {tab === 'webhooks' && (
      <>
      <div className="mb-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="font-mono text-[11px] tracking-[1px] text-muted">WEBHOOKS</span>
          <button className="secondary" onClick={loadWebhooks}><RefreshCw size={12} />Refresh</button>
        </div>

        {health?.urlMismatch && (
          <div className="mb-4 rounded-[16px] border border-[#f87171]/50 bg-panel p-4">
            <div className="flex items-center gap-2 text-[12px] font-semibold text-[#f87171]">
              <i className="inline-block h-2 w-2 rounded-full bg-[#f87171]" />
              Webhook points at a STALE tunnel URL
            </div>
            <p className="mt-1.5 text-[11px] leading-[1.6] text-muted">
              Telegram is still sending to <code className="font-mono text-[11px]">{health.telegram?.url}</code> but your live tunnel is <code className="font-mono text-[11px]">{health.liveTunnel}</code>. Re-register the webhook so Telegram targets the current tunnel.
            </p>
          </div>
        )}

        {health && (
          <div className="mb-4 grid max-w-[1100px] grid-cols-1 gap-3 md:grid-cols-3">
            <div className={`rounded-[16px] border p-4 ${health.receiverListening ? 'border-[var(--green)]/50' : 'border-[#f87171]/50'}`}>
              <div className="flex items-center gap-2 text-[11px] font-semibold">
                <i className={`inline-block h-2 w-2 rounded-full ${health.receiverListening ? 'bg-[var(--green)]' : 'bg-[#f87171]'}`} />
                Webhook receiver (port 14789)
              </div>
              <p className="mt-1 text-[11px] text-muted">{health.receiverListening ? 'Listening — cloudflared can forward here.' : 'NOT listening — the local receiver failed to bind or is not running.'}</p>
            </div>
            <div className={`rounded-[16px] border p-4 ${health.tunnelUrl ? 'border-[var(--green)]/50' : 'border-[#f87171]/50'}`}>
              <div className="flex items-center gap-2 text-[11px] font-semibold">
                <i className={`inline-block h-2 w-2 rounded-full ${health.tunnelUrl ? 'bg-[var(--green)]' : 'bg-[#f87171]'}`} />
                Tunnel
              </div>
              <p className="mt-1 truncate font-mono text-[10.5px] text-muted">{health.tunnelUrl ?? 'No tunnel running'}</p>
            </div>
            <div className={`rounded-[16px] border p-4 ${health.webhookRegistered ? 'border-[var(--green)]/50' : 'border-[#facc15]/50'}`}>
              <div className="flex items-center gap-2 text-[11px] font-semibold">
                <i className={`inline-block h-2 w-2 rounded-full ${health.webhookRegistered ? 'bg-[var(--green)]' : 'bg-[#facc15]'}`} />
                Telegram webhook
              </div>
              <p className="mt-1 truncate font-mono text-[10.5px] text-muted">{health.telegram?.url ?? (health.webhookRegistered ? 'registered' : 'not registered')}</p>
              {typeof health.telegram?.pending_update_count === 'number' && health.telegram.pending_update_count > 0 && (
                <p className="mt-1 text-[11px] text-[#facc15]">{health.telegram.pending_update_count} pending update(s) — messages queued but not delivered</p>
              )}
              {health.telegram?.last_error_message && (
                <p className="mt-1 text-[11px] text-[#f87171]">last error: {health.telegram.last_error_message}</p>
              )}
            </div>
          </div>
        )}

        <div className="runs-console max-h-[420px] overflow-y-auto rounded-[16px] border border-line bg-inset p-3.5 font-mono text-[12px] leading-[1.6]">
          {tLogs.length === 0 ? (
            <div className="console-empty p-2.5 text-center text-[12px] text-muted">
              <p>No Telegram activity yet. Messages routed via the webhook or polling show up here.</p>
            </div>
          ) : tLogs.map((l, i) => (
            <div key={i} className="border-b border-[#1c1c1f] py-2 last:border-0">
              <div className="flex items-center gap-3 text-[11px]">
                <span className={l.direction === 'in' ? 'text-[#38bdf8]' : l.direction === 'out' ? 'text-[#22c55e]' : 'text-[#facc15]'}>{l.direction === 'in' ? '▸ IN' : l.direction === 'out' ? '◂ OUT' : '■ SYS'}</span>
                <span className={`font-mono text-[11px] ${l.status === 'error' ? 'text-[#f87171]' : l.status === 'sent' ? 'text-[#22c55e]' : 'text-[#facc15]'}`}>{l.status}</span>
                {l.chatId && <span className="text-mid">chat {l.chatId}</span>}
                <span className="ml-auto text-mid">{l.createdAt ? (() => { const d = new Date(l.createdAt); return isNaN(d.getTime()) ? '' : d.toLocaleTimeString(); })() : ''}</span>
              </div>
              {l.text && <div className="mt-1 text-[#d4d4d8]">in: {l.text}</div>}
              {l.reply && <div className="mt-0.5 text-[#a1a1aa]">out: {l.reply.length > 300 ? `${l.reply.slice(0, 300)}…` : l.reply}</div>}
              {l.detail && <div className="mt-0.5 text-[#f87171]">{l.detail}</div>}
            </div>
          ))}
        </div>
      </div>
      </>
      )}
    </>
  );
}
