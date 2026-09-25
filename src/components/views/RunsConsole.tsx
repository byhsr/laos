import { useEffect, useState } from 'react';
import { ArrowDownLeft, ArrowUpRight, ChevronRight, RefreshCw } from 'lucide-react';
import type { Agent, Run } from '../../types';
import { listTelegramLogs, telegramWebhookHealth, type TelegramLogEntry, type WebhookHealth } from '../../runtime';
import { Button } from '../ui/Button';
import { StatusGlyph, StatusTag, statusClass } from '../ui/Status';
import { tabCls } from '../ui/tabs';

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

// Live activity, presented as a command-line-style feed: newest first, one
// summary line per run, details hidden until the row is expanded.
export function RunsConsole({ runs, agents, onOpenAgent, onClear }: {
  runs: Run[]; agents: Agent[]; onOpenAgent: (id: string) => void; onClear: () => void;
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

  const section = 'font-mono text-[10px] tracking-wider text-muted uppercase';
  const healthCard = (ok: boolean | undefined) => `rounded-xl border p-3.5 ${ok ? 'border-border' : 'border-danger/50'}`;
  const dot = (ok: boolean | undefined) => `inline-block h-1.5 w-1.5 shrink-0 rounded-full ${ok ? 'bg-foreground' : 'bg-danger'}`;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* No view title — the header row carries only the tabs and the action. */}
      <div className="mb-4 flex shrink-0 flex-wrap items-center gap-1">
        <button className={tabCls(tab === 'logs')} onClick={() => setTab('logs')}>logs</button>
        <button className={tabCls(tab === 'webhooks')} onClick={() => setTab('webhooks')}>
          webhooks
          {health?.receiverListening === false && <i className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-danger" />}
        </button>
        {tab === 'logs' && (
          <Button className="ml-auto" onClick={onClear} disabled={runs.length === 0}>clear log</Button>
        )}
      </div>

      {tab === 'logs' && (
        <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-background p-3 font-mono text-[11px] leading-relaxed">
          {runs.length === 0 && (
            <p className="m-0 p-2.5 text-center font-mono text-[11px] text-muted">no runs yet — open an agent and send a task, and every step shows up here like a live command line</p>
          )}
          {[...runs].sort((a, b) => new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()).map((r) => {
            const isOpen = expanded.has(r.id);
            const totalTokens = (r.promptTokens ?? 0) + (r.completionTokens ?? 0);
            return (
              <div key={r.id} className="border-b border-border last:border-0">
                {/* Collapsed log row */}
                <div
                  role="button"
                  tabIndex={0}
                  className="focus-ring flex w-full cursor-pointer items-center gap-3 rounded px-1 py-2 text-left transition-colors hover:bg-surface"
                  onClick={() => toggle(r.id)}
                  onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(r.id); } }}
                >
                  <ChevronRight size={11} className={`shrink-0 text-muted transition-transform duration-150 ${isOpen ? 'rotate-90' : ''}`} />
                  <span className={`shrink-0 ${statusClass(r.status)}`}><StatusGlyph status={r.status} /></span>
                  <span className="shrink-0 text-muted">{fmtTime(r.startedAt)}</span>
                  <button
                    className="focus-ring shrink-0 cursor-pointer truncate rounded border-0 bg-transparent p-0 font-mono text-[11px] text-foreground hover:underline"
                    onClick={(e) => { e.stopPropagation(); onOpenAgent(r.agentId); }}
                  >
                    {agentName(r.agentId)}
                  </button>
                  <span className="min-w-0 truncate text-muted">{r.model}</span>
                  <span className="shrink-0 text-muted">{fmtDuration(r)}</span>
                  {totalTokens > 0 && <span className="shrink-0 text-muted">{totalTokens.toLocaleString()} tok</span>}
                  <StatusTag status={r.status} className="ml-auto" />
                </div>

                {/* Expanded details — hidden until the user opens the row */}
                {isOpen && (
                  <div className="px-1 pb-3">
                    <div className="my-1 text-foreground/80">$ {r.input}</div>
                    {(r.events ?? []).map((ev, i) => (
                      <div key={i} className="flex items-baseline gap-2">
                        <span className="flex-none text-muted">{ev.time}</span>
                        <span className="w-10 flex-none text-muted">{ev.type === 'tool' ? 'tool' : ev.type === 'thought' ? 'think' : 'out'}</span>
                        <span className="min-w-0 flex-1 text-foreground/70">{ev.title}{ev.detail ? ` — ${ev.detail}` : ''}</span>
                      </div>
                    ))}
                    {r.output && <pre className="mt-1.5 ml-12 max-w-full overflow-x-hidden whitespace-pre-wrap rounded border border-border bg-background p-2 text-[11px] text-foreground">{r.output}</pre>}
                    {r.status === 'failed' && (
                      <div className="flex items-baseline gap-2">
                        <span className="flex-none text-muted" />
                        <span className="w-10 flex-none text-danger">err</span>
                        <span className="min-w-0 flex-1 text-muted">run failed — see agent chat for details.</span>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {tab === 'webhooks' && (
        <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
          <div className="mb-3 flex items-center justify-between gap-3">
            <span className={section}>delivery chain</span>
            <Button icon={<RefreshCw size={12} />} onClick={loadWebhooks}>refresh</Button>
          </div>

          {health?.urlMismatch && (
            <div className="mb-3 rounded-xl border border-danger/50 bg-surface p-4">
              <div className="flex items-center gap-2 font-mono text-[11px] text-danger">
                <i className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-danger" />
                webhook points at a stale tunnel url
              </div>
              <p className="mt-1.5 mb-0 text-[11px] leading-relaxed text-muted">
                Telegram is still sending to <code className="font-mono">{health.telegram?.url}</code> but your live tunnel is <code className="font-mono">{health.liveTunnel}</code>. Re-register the webhook so Telegram targets the current tunnel.
              </p>
            </div>
          )}

          {health && (
            <div className="mb-3 grid grid-cols-1 gap-2.5 md:grid-cols-3">
              <div className={healthCard(health.receiverListening)}>
                <div className="flex items-center gap-2 font-mono text-[11px] text-foreground">
                  <i className={dot(health.receiverListening)} />
                  webhook receiver (port 14789)
                </div>
                <p className="mt-1 mb-0 text-[11px] leading-relaxed text-muted">{health.receiverListening ? 'Listening — cloudflared can forward here.' : 'NOT listening — the local receiver failed to bind or is not running.'}</p>
              </div>
              <div className={healthCard(!!health.tunnelUrl)}>
                <div className="flex items-center gap-2 font-mono text-[11px] text-foreground">
                  <i className={dot(!!health.tunnelUrl)} />
                  tunnel
                </div>
                <p className="mt-1 mb-0 truncate font-mono text-[10px] text-muted">{health.tunnelUrl ?? 'no tunnel running'}</p>
              </div>
              <div className={healthCard(health.webhookRegistered)}>
                <div className="flex items-center gap-2 font-mono text-[11px] text-foreground">
                  <i className={dot(health.webhookRegistered)} />
                  telegram webhook
                </div>
                <p className="mt-1 mb-0 truncate font-mono text-[10px] text-muted">{health.telegram?.url ?? (health.webhookRegistered ? 'registered' : 'not registered')}</p>
                {typeof health.telegram?.pending_update_count === 'number' && health.telegram.pending_update_count > 0 && (
                  <p className="mt-1 mb-0 text-[11px] text-muted">{health.telegram.pending_update_count} pending update(s) — messages queued but not delivered</p>
                )}
                {health.telegram?.last_error_message && (
                  <p className="mt-1 mb-0 text-[11px] text-danger">last error: {health.telegram.last_error_message}</p>
                )}
              </div>
            </div>
          )}

          <span className={section}>telegram feed</span>
          <div className="mt-2 max-h-[420px] overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-background p-3 font-mono text-[11px] leading-relaxed">
            {tLogs.length === 0 ? (
              <p className="m-0 p-2.5 text-center text-muted">no Telegram activity yet — messages routed via the webhook or polling show up here</p>
            ) : tLogs.map((l, i) => (
              <div key={i} className="border-b border-border py-2 last:border-0">
                <div className="flex items-center gap-3">
                  <span className="flex shrink-0 items-center gap-1 text-muted">
                    {l.direction === 'in'
                      ? <><ArrowDownLeft size={11} />in</>
                      : l.direction === 'out'
                        ? <><ArrowUpRight size={11} />out</>
                        : 'sys'}
                  </span>
                  <span className={`shrink-0 lowercase ${l.status === 'error' ? 'text-danger' : 'text-muted'}`}>{l.status}</span>
                  {l.chatId && <span className="shrink-0 text-muted">chat {l.chatId}</span>}
                  <span className="ml-auto shrink-0 text-muted">{l.createdAt ? (() => { const d = new Date(l.createdAt); return isNaN(d.getTime()) ? '' : d.toLocaleTimeString(); })() : ''}</span>
                </div>
                {l.text && <div className="mt-1 break-words text-foreground/80">in: {l.text}</div>}
                {l.reply && <div className="mt-0.5 break-words text-muted">out: {l.reply.length > 300 ? `${l.reply.slice(0, 300)}…` : l.reply}</div>}
                {l.detail && <div className="mt-0.5 break-words text-danger">{l.detail}</div>}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
