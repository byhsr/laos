import { useEffect } from 'react';
import { Bell, Check, CheckCheck, Trash2 } from 'lucide-react';
import { Button, IconButton } from '../ui/Button';
import { useNotifications } from '../../hooks/useNotifications';

// When it arrived: the clock time if today, else the date.
const fmtWhen = (iso: string) => {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  const sameDay = d.toDateString() === new Date().toDateString();
  return sameDay
    ? d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    : d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
};

// The notification inbox: everything the app wanted to tell the user — reminders
// that fired, insights to review, finished tasks and workflow results. Persisted
// so nothing is lost to a closed window or a 5-second toast.
export function NotificationsView() {
  const items = useNotifications((s) => s.items);
  const unread = useNotifications((s) => s.unread);
  const load = useNotifications((s) => s.load);
  const markRead = useNotifications((s) => s.markRead);
  const dismiss = useNotifications((s) => s.dismiss);
  const markAllRead = useNotifications((s) => s.markAllRead);
  const clear = useNotifications((s) => s.clear);

  useEffect(() => { void load(); }, [load]);

  return (
    <div className="flex h-full flex-col">
      <div className="mb-4 flex shrink-0 items-center gap-2">
        <span className="font-mono text-[11px] text-muted">{unread > 0 ? `${unread} unread` : 'all caught up'}</span>
        <div className="ml-auto flex items-center gap-2">
          <Button icon={<CheckCheck size={12} />} onClick={() => void markAllRead()} disabled={unread === 0}>mark all read</Button>
          <Button icon={<Trash2 size={12} />} onClick={() => void clear()} disabled={items.length === 0}>clear</Button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {items.length === 0 && (
          <div className="grid place-items-center rounded-xl border border-dashed border-border p-6 text-center text-muted">
            <Bell size={20} className="mb-2 opacity-50" />
            <p className="m-0 text-[12px]">no notifications — reminders, insights and finished work land here.</p>
          </div>
        )}
        <div className="grid gap-1.5">
          {items.map((n) => (
            <div
              key={n.id}
              className={`group flex items-start gap-2.5 rounded-lg border border-border bg-surface px-2.5 py-2 ${n.readAt ? '' : 'border-l-2 border-l-foreground/40'}`}
            >
              <span className="shrink-0 rounded border border-border px-1.5 py-0.5 font-mono text-[9px] tracking-wider text-muted uppercase">{n.kind}</span>
              <div className="min-w-0 flex-1">
                <b className="block font-mono text-[11px] text-foreground">{n.title}</b>
                {n.body && <span className="mt-0.5 block text-[11px] text-muted">{n.body}</span>}
              </div>
              {n.createdAt && (
                <span className="shrink-0 font-mono text-[10px] text-muted/70" title={new Date(n.createdAt).toLocaleString()}>{fmtWhen(n.createdAt)}</span>
              )}
              <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
                {!n.readAt && <IconButton label="mark read" onClick={() => void markRead(n.id)}><Check size={12} /></IconButton>}
                <IconButton label="dismiss" onClick={() => void dismiss(n.id)}><Trash2 size={11} /></IconButton>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
