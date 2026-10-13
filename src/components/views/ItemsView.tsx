import { useEffect, useState } from 'react';
import { Check, Inbox, Plus, Sparkles, Trash2, X } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button, IconButton } from '../ui/Button';
import { INPUT_CLS } from '../ui/Input';
import { Select } from '../ui/Select';
import { tabCls } from '../ui/tabs';
import { toast } from '../../hooks/useToast';
import {
  ITEM_KINDS, insightAccept, insightDismiss, insightList, itemDelete, itemList, itemSave, itemToggle,
  listDestinations, pushItem, reflectNow, reminderSnooze, type Destination, type Insight, type Item,
} from '../../runtime';

const fmtDue = (iso: string | null) => {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  const late = d.getTime() < Date.now();
  return `${late ? '⚠ ' : ''}${d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}`;
};

// The Lumin inbox: everything captured — notes, todos, reminders, promises, links
// and habits — plus the insights the learning layer proposes.
export function ItemsView() {
  const [items, setItems] = useState<Item[]>([]);
  const [insights, setInsights] = useState<Insight[]>([]);
  const [destinations, setDestinations] = useState<Destination[]>([]);
  const [filter, setFilter] = useState<string>('all');
  const [kind, setKind] = useState<string>('note');
  const [text, setText] = useState('');
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);

  const showInsights = filter === 'insights';
  const load = async () => {
    try {
      if (showInsights) setInsights(await insightList('new'));
      else setItems(await itemList(filter === 'all' ? undefined : filter));
    } catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };
  useEffect(() => { void load(); }, [filter]);
  useEffect(() => { listDestinations().then(setDestinations); }, []);

  // Push one item to a destination (e.g. a Notion account via an MCP server).
  const sendTo = async (it: Item, destinationId: string) => {
    if (!destinationId) return;
    setBusy(true);
    try { toast(await pushItem(it.id, destinationId), 'success'); }
    catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
    finally { setBusy(false); }
  };

  const add = async () => {
    const body = text.trim();
    if (!body) return;
    const words = body.split(/\s+/);
    const url = words.find((w) => /^https?:\/\//.test(w)) ?? '';
    const tags = words.filter((w) => w.startsWith('#') && w.length > 1).map((w) => w.slice(1));
    try {
      await itemSave({
        id: '', kind, title: words.slice(0, 7).join(' ') + (words.length > 7 ? '…' : ''), body, url,
        tags, collection: tags[0] ?? '', status: 'active', due_at: null,
        cadence: kind === 'habit' ? 'daily' : '', streak: 0, last_done_at: null, project_id: '',
        fired_at: null, source: 'app', created_at: '', updated_at: '',
      });
      setText('');
      await load();
    } catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };

  const toggle = async (it: Item) => {
    try { toast(await itemToggle(it.id), 'success'); await load(); }
    catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };

  const reflect = async () => {
    setBusy(true);
    try {
      const n = await reflectNow();
      await load();
      toast(n ? `${n} new insight${n === 1 ? '' : 's'}` : 'nothing new to notice', 'success');
    } catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
    finally { setBusy(false); }
  };

  // Search across title, body and tags — a captured list gets long fast.
  const q = query.trim().toLowerCase();
  const visible = q
    ? items.filter((it) => [it.title, it.body, ...it.tags].join(' ').toLowerCase().includes(q))
    : items;

  return (
    <div className="flex h-full flex-col">
      <div className="mb-4 flex shrink-0 flex-wrap items-center gap-1">
        {['all', ...ITEM_KINDS, 'insights'].map((k) => (
          <button key={k} className={tabCls(filter === k)} onClick={() => setFilter(k)}>{k}</button>
        ))}
        {!showInsights && (
          <div className="ml-auto w-[180px]">
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="search…"
              className={INPUT_CLS}
            />
          </div>
        )}
      </div>

      {showInsights ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mb-3 flex items-center gap-2">
            <Button icon={<Sparkles size={12} />} onClick={() => void reflect()} disabled={busy}>{busy ? 'thinking…' : 'reflect now'}</Button>
            <span className="font-mono text-[10px] text-muted">what the assistant noticed — accept or dismiss</span>
          </div>
          {insights.length === 0 && (
            <div className="grid place-items-center rounded-xl border border-dashed border-border p-6 text-center text-muted">
              <Sparkles size={20} className="mb-2 opacity-50" />
              <p className="m-0 text-[12px]">no new insights — they appear as you file things.</p>
            </div>
          )}
          <div className="grid gap-1.5">
            {insights.map((ins) => (
              <div key={ins.id} className="flex items-start gap-2.5 rounded-lg border border-border bg-surface px-2.5 py-2">
                <span className="shrink-0 rounded border border-border px-1.5 py-0.5 font-mono text-[9px] tracking-wider text-muted uppercase">{ins.kind.replace('_', ' ')}</span>
                <div className="min-w-0 flex-1">
                  <b className="block font-mono text-[11px] text-foreground">{ins.title}</b>
                  {ins.body && <span className="mt-0.5 block text-[11px] text-muted">{ins.body}</span>}
                </div>
                <IconButton label="accept" onClick={() => void insightAccept(ins.id).then(load)}><Check size={12} /></IconButton>
                <IconButton label="dismiss" onClick={() => void insightDismiss(ins.id).then(load)}><X size={12} /></IconButton>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <>
          <Card className="mb-3 shrink-0">
            <div className="flex flex-wrap items-end gap-2">
              <div className="min-w-[120px]">
                <Select value={kind} options={ITEM_KINDS.map((k) => ({ value: k, label: k }))} onChange={setKind} />
              </div>
              <div className="min-w-[220px] flex-1">
                <input
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  onKeyDown={(e) => { if (e.key === 'Enter') void add(); }}
                  placeholder="file something — a thought, a todo, a link…  (#tags work)"
                  className={INPUT_CLS}
                />
              </div>
              <Button variant="primary" icon={<Plus size={12} />} onClick={() => void add()}>add</Button>
            </div>
          </Card>

          <div className="min-h-0 flex-1 overflow-y-auto">
            {visible.length === 0 && (
              <div className="grid place-items-center rounded-xl border border-dashed border-border p-6 text-center text-muted">
                <Inbox size={20} className="mb-2 opacity-50" />
                <p className="m-0 text-[12px]">
                  {q
                    ? `no items match “${query.trim()}”.`
                    : filter === 'all'
                      ? 'nothing saved yet — file items here, or text them to a bot set to capture free text.'
                      : `no ${filter} items yet.`}
                </p>
              </div>
            )}
            <div className="grid gap-1.5">
              {visible.map((it) => (
                <div key={it.id} className="group flex items-center gap-2.5 rounded-lg border border-border bg-surface px-2.5 py-2">
                  <button
                    className={`focus-ring grid h-4 w-4 shrink-0 cursor-pointer place-items-center rounded border font-mono text-[9px] ${it.status === 'done' ? 'border-foreground/40 bg-foreground/10 text-foreground' : 'border-border text-transparent hover:border-foreground/30'}`}
                    onClick={() => void toggle(it)}
                    aria-label={it.status === 'done' ? 'mark active' : 'mark done'}
                  >✓</button>
                  <span className="shrink-0 rounded border border-border px-1.5 py-0.5 font-mono text-[9px] tracking-wider text-muted uppercase">{it.kind}</span>
                  <div className="min-w-0 flex-1">
                    <span className={`block truncate font-mono text-[11px] ${it.status === 'done' ? 'text-muted line-through' : 'text-foreground'}`}>{it.title || it.body}</span>
                    {/* The title is capped at 7 words; show the body's opening so a
                        note is readable at a glance without opening it. */}
                    {it.title.trim() && it.body.trim() && it.body.trim() !== it.title.trim() && (
                      <span className="block truncate font-mono text-[10px] text-muted">{it.body}</span>
                    )}
                  </div>
                  {it.streak > 0 && <span className="shrink-0 font-mono text-[10px] text-muted">{it.streak}🔥</span>}
                  {it.due_at && <span className="shrink-0 font-mono text-[10px] text-muted">{fmtDue(it.due_at)}</span>}
                  {/* The title keeps the row; the rest appears on hover so a row
                      with tags and a destination can't crush it. */}
                  <div className="flex shrink-0 items-center gap-2 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
                    {it.tags.slice(0, 3).map((t) => <span key={t} className="font-mono text-[10px] text-muted">#{t}</span>)}
                    {it.url && <a href={it.url} target="_blank" rel="noreferrer" className="font-mono text-[10px] text-muted hover:text-foreground">link</a>}
                    {it.kind === 'reminder' && it.due_at && (
                      <button className="cursor-pointer font-mono text-[10px] text-muted hover:text-foreground" onClick={() => void reminderSnooze(it.id).then(load)}>snooze</button>
                    )}
                    {destinations.length > 0 && (
                      <div className="w-[104px]">
                        <Select
                          value=""
                          options={destinations.map((d) => ({ value: d.id, label: d.name || d.tool }))}
                          onChange={(v) => void sendTo(it, v)}
                          placeholder="send to ▸"
                        />
                      </div>
                    )}
                    <IconButton label="delete" onClick={() => void itemDelete(it.id).then(load)}><Trash2 size={11} /></IconButton>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
