import { useEffect, useState } from 'react';
import { ArrowDownLeft, ArrowUpRight, Bot, Check, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { listTelegramLogs, listTelegramBots, saveTelegramBot, deleteTelegramBot, setBotWebhook, clearBotWebhook, terminalList, type TelegramLogEntry, type TelegramBot, type TerminalSession } from '../../runtime';
import { useAgentsStore } from '../../hooks/useAgents';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { Select } from '../ui/Select';
import { Checkbox } from '../ui/Checkbox';
import { FIELD_LABEL_CLS, INPUT_CLS } from '../ui/Input';
import { toast } from '../../hooks/useToast';

const fmtTime = (s?: string | null) => {
  if (!s) return '';
  const d = new Date(s);
  return isNaN(d.getTime()) ? '' : d.toLocaleTimeString();
};

const blankBot = (): TelegramBot => ({ id: '', name: '', agentId: 'manager', terminalId: '', enabled: true, token: '', webhookRegistered: false });

// Telegram: manage multiple bots (each long-polled and routed to an agent), plus
// the activity log. Polls the log while the view is open.
export function TelegramView() {
  const agents = useAgentsStore((s) => s.agents);
  const [logs, setLogs] = useState<TelegramLogEntry[]>([]);
  const [bots, setBots] = useState<TelegramBot[]>([]);
  const [editing, setEditing] = useState<TelegramBot | null>(null);
  const [target, setTarget] = useState<'agent' | 'terminal'>('agent');
  const [terminals, setTerminals] = useState<TerminalSession[]>([]);
  const [saving, setSaving] = useState(false);

  const load = async () => setLogs(await listTelegramLogs());
  const loadBots = async () => { setBots(await listTelegramBots()); setTerminals(await terminalList()); };

  const openEdit = (b: TelegramBot) => { setEditing(b); setTarget(b.terminalId ? 'terminal' : 'agent'); };

  useEffect(() => {
    load();
    loadBots();
    const iv = setInterval(load, 3000);
    return () => clearInterval(iv);
  }, []);

  const agentOptions = [
    { value: 'manager', label: agents.find((a) => a.isManager)?.name || 'Manager' },
    ...agents.filter((a) => !a.isManager).map((a) => ({ value: a.id, label: a.name })),
  ];
  const agentName = (id: string) => agentOptions.find((o) => o.value === id)?.label ?? id;

  const saveBot = async () => {
    if (!editing) return;
    if (!editing.name.trim()) { toast('Bot needs a name', 'error'); return; }
    setSaving(true);
    try {
      await saveTelegramBot({ ...editing, name: editing.name.trim() });
      setEditing(null);
      await loadBots();
      toast('bot saved', 'success');
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="shrink-0">
        <div className="mb-2 flex items-center justify-between">
          <span className="font-mono text-[10px] tracking-wider text-muted uppercase">bots</span>
          <Button icon={<Plus size={12} />} onClick={() => openEdit(blankBot())}>add bot</Button>
        </div>

        <div className="grid gap-2">
          {bots.map((b) => (
            <Card key={b.id} className="flex items-center gap-3">
              <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-background text-muted"><Bot size={14} /></span>
              <div className="min-w-0 flex-1">
                <b className="block truncate font-mono text-[11px] text-foreground">{b.name}</b>
                <span className="block truncate font-mono text-[10px] text-muted">→ {b.terminalId ? `terminal: ${terminals.find((t) => t.id === b.terminalId)?.name ?? b.terminalId}` : `agent: ${agentName(b.agentId)}`}{b.token ? ' · token set' : ' · no token'}{b.enabled ? '' : ' · disabled'}{b.webhookRegistered ? ' · webhook' : ' · long-poll'}</span>
              </div>
              <div className="flex shrink-0 items-center gap-1.5">
                {b.id && (
                  <Button onClick={async () => {
                    try {
                      if (b.webhookRegistered) { await clearBotWebhook(b.id); toast('webhook cleared — long-polling', 'success'); }
                      else { await setBotWebhook(b.id); toast('webhook registered', 'success'); }
                      await loadBots();
                    } catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
                  }}>{b.webhookRegistered ? 'webhook off' : 'webhook on'}</Button>
                )}
                <Button onClick={() => openEdit({ ...b })}>edit</Button>
                <IconButton label="delete bot" onClick={async () => { await deleteTelegramBot(b.id); await loadBots(); }}><Trash2 size={12} /></IconButton>
              </div>
            </Card>
          ))}
          {bots.length === 0 && <p className="m-0 font-mono text-[10px] text-muted">no bots yet — add one with your BotFather token. Existing single-bot tokens were migrated automatically.</p>}
        </div>

        {editing && (
          <Card className="mt-2 grid gap-2">
            <label className={FIELD_LABEL_CLS}>name</label>
            <input value={editing.name} onChange={(e) => setEditing({ ...editing, name: e.target.value })} placeholder="e.g. Support bot" className={INPUT_CLS} />
            <label className={`${FIELD_LABEL_CLS} mt-2`}>bot token</label>
            <input type="password" value={editing.token} onChange={(e) => setEditing({ ...editing, token: e.target.value })} placeholder="123456:ABC…" className={INPUT_CLS} />
            <label className={`${FIELD_LABEL_CLS} mt-2`}>target</label>
            <Select
              value={target}
              options={[{ value: 'agent', label: 'an agent' }, { value: 'terminal', label: 'a terminal session' }]}
              onChange={(v) => { const t = v as 'agent' | 'terminal'; setTarget(t); setEditing(t === 'agent' ? { ...editing, terminalId: '' } : { ...editing, terminalId: terminals[0]?.id ?? '' }); }}
            />
            {target === 'agent' ? (
              <Select value={editing.agentId} options={agentOptions} onChange={(v) => setEditing({ ...editing, agentId: v })} />
            ) : (
              <Select
                value={editing.terminalId}
                options={terminals.map((t) => ({ value: t.id, label: `${t.name}${t.alive ? '' : ' (exited)'}` }))}
                onChange={(v) => setEditing({ ...editing, terminalId: v })}
                placeholder="select a session…"
              />
            )}
            <div className="-mx-2 mt-1.5">
              <Checkbox checked={editing.enabled} onChange={(next) => setEditing({ ...editing, enabled: next })} label="enabled" />
            </div>
            <div className="mt-1 flex justify-end gap-2">
              <Button onClick={() => setEditing(null)}>cancel</Button>
              <Button variant="primary" icon={<Check size={12} />} disabled={saving} onClick={saveBot}>{saving ? 'saving…' : 'save'}</Button>
            </div>
          </Card>
        )}
      </div>

      <div className="flex shrink-0 items-center justify-between">
        <span className="font-mono text-[10px] tracking-wider text-muted uppercase">activity</span>
        <Button icon={<RefreshCw size={12} />} onClick={load}>refresh</Button>
      </div>

      <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-background p-3 font-mono text-[11px] leading-relaxed">
        {logs.length === 0 ? (
          <p className="m-0 font-mono text-[10px] text-muted">No Telegram activity yet.</p>
        ) : logs.map((l, i) => (
          <div key={i} className="border-b border-border py-2 last:border-0">
            <div className="flex items-center gap-3">
              <span className="flex shrink-0 items-center gap-1 text-muted">
                {l.direction === 'in'
                  ? <><ArrowDownLeft size={11} />in</>
                  : <><ArrowUpRight size={11} />out</>}
              </span>
              <span className={`shrink-0 lowercase ${l.status === 'error' ? 'text-danger' : 'text-muted'}`}>{l.status}</span>
              {l.detail && <span className="shrink-0 text-muted">{l.detail}</span>}
              <span className="ml-auto shrink-0 text-muted">{fmtTime(l.createdAt)}</span>
            </div>
            <div className="mt-1 break-words text-foreground/80">in: {l.text}</div>
            {l.reply && <div className="mt-0.5 break-words text-muted">out: {l.reply.length > 300 ? `${l.reply.slice(0, 300)}…` : l.reply}</div>}
          </div>
        ))}
      </div>
    </div>
  );
}
