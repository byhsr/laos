import { useEffect, useState } from 'react';
import { ArrowDownLeft, ArrowUpRight, RefreshCw } from 'lucide-react';
import { listTelegramLogs, type TelegramLogEntry } from '../../runtime';
import { Button } from '../ui/Button';

const fmtTime = (s?: string | null) => {
  if (!s) return '';
  const d = new Date(s);
  return isNaN(d.getTime()) ? '' : d.toLocaleTimeString();
};

// Telegram activity log, promoted to its own topbar section (it used to live in a
// Laos sub-tab). Polls while the view is open.
export function TelegramView() {
  const [logs, setLogs] = useState<TelegramLogEntry[]>([]);

  const load = async () => setLogs(await listTelegramLogs());

  useEffect(() => {
    load();
    const iv = setInterval(load, 3000);
    return () => clearInterval(iv);
  }, []);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="mb-3 flex shrink-0 items-center justify-end">
        <Button icon={<RefreshCw size={12} />} onClick={load}>refresh</Button>
      </div>

      <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-background p-3 font-mono text-[11px] leading-relaxed">
        {logs.length === 0 ? (
          <p className="m-0 font-mono text-[10px] text-muted">No Telegram activity yet. Set up the bot token in Workshop → Integrations.</p>
        ) : logs.map((l, i) => (
          <div key={i} className="border-b border-border py-2 last:border-0">
            <div className="flex items-center gap-3">
              <span className="flex shrink-0 items-center gap-1 text-muted">
                {l.direction === 'in'
                  ? <><ArrowDownLeft size={11} />in</>
                  : <><ArrowUpRight size={11} />out</>}
              </span>
              <span className={`shrink-0 lowercase ${l.status === 'error' ? 'text-danger' : 'text-muted'}`}>{l.status}</span>
              <span className="ml-auto shrink-0 text-muted">{fmtTime(l.createdAt)}</span>
            </div>
            <div className="mt-1 break-words text-foreground/80">in: {l.text}</div>
            {l.reply && <div className="mt-0.5 break-words text-muted">out: {l.reply.length > 300 ? `${l.reply.slice(0, 300)}…` : l.reply}</div>}
            {l.detail && <div className="mt-0.5 break-words text-danger">detail: {l.detail}</div>}
          </div>
        ))}
      </div>
    </div>
  );
}
