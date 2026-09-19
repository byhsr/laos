import { useEffect, useState } from 'react';
import { RefreshCw, Send } from 'lucide-react';
import { listTelegramLogs, type TelegramLogEntry } from '../../runtime';

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
      <header className="mb-4 flex shrink-0 items-center justify-between">
        <div className="flex items-center gap-1.5">
          <Send size={14} className="text-[var(--green)]" />
          <span className="font-mono text-[11px] uppercase tracking-[1px] text-text">Telegram</span>
        </div>
        <button className="secondary" onClick={load}><RefreshCw size={12} />Refresh</button>
      </header>

      <div className="runs-console min-h-0 flex-1 overflow-y-auto rounded-lg border border-line bg-inset p-3.5 font-mono text-[12px] leading-[1.6]">
        {logs.length === 0 ? (
          <p className="text-[11px] text-muted">No Telegram activity yet. Set up the bot token in Workshop → Integrations.</p>
        ) : logs.map((l, i) => (
          <div key={i} className="border-b border-[#1c1c1f] py-2 last:border-0">
            <div className="flex items-center gap-3 text-[11px]">
              <span className={l.direction === 'in' ? 'text-[#38bdf8]' : 'text-[#22c55e]'}>{l.direction === 'in' ? '▸ IN' : '◂ OUT'}</span>
              <span className={`font-mono text-[11px] ${l.status === 'error' ? 'text-[#f87171]' : l.status === 'sent' ? 'text-[#22c55e]' : 'text-[#facc15]'}`}>{l.status}</span>
              <span className="ml-auto text-mid">{fmtTime(l.createdAt)}</span>
            </div>
            <div className="mt-1 text-[#d4d4d8]">in: {l.text}</div>
            {l.reply && <div className="mt-0.5 text-[#a1a1aa]">out: {l.reply.length > 300 ? `${l.reply.slice(0, 300)}…` : l.reply}</div>}
            {l.detail && <div className="mt-0.5 text-[#f87171]">detail: {l.detail}</div>}
          </div>
        ))}
      </div>
    </div>
  );
}
