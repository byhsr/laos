import { useEffect, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button, IconButton } from '../ui/Button';
import { INPUT_CLS } from '../ui/Input';
import { tabCls } from '../ui/tabs';
import { toast } from '../../hooks/useToast';
import { ITEM_KINDS, itemDelete, itemList, itemSave, itemToggle, type Item } from '../../runtime';

// The second-brain inbox: everything filed from Telegram (and here) — notes,
// todos, reminders, promises and links — in one list.
export function ItemsView() {
  const [items, setItems] = useState<Item[]>([]);
  const [filter, setFilter] = useState<string>('all');
  const [kind, setKind] = useState<string>('note');
  const [text, setText] = useState('');

  const load = async () => {
    try { setItems(await itemList(filter === 'all' ? undefined : filter)); }
    catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };
  useEffect(() => { void load(); }, [filter]);

  const add = async () => {
    const body = text.trim();
    if (!body) return;
    const words = body.split(/\s+/);
    const url = words.find((w) => /^https?:\/\//.test(w)) ?? '';
    try {
      await itemSave({
        id: '', kind, title: words.slice(0, 7).join(' ') + (words.length > 7 ? '…' : ''), body, url,
        tags: [], collection: '', status: 'active', due_at: null, source: 'app', created_at: '', updated_at: '',
      });
      setText('');
      await load();
    } catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="mb-4 flex shrink-0 flex-wrap items-center gap-1">
        {['all', ...ITEM_KINDS].map((k) => (
          <button key={k} className={tabCls(filter === k)} onClick={() => setFilter(k)}>{k}</button>
        ))}
      </div>

      <Card className="mb-3 shrink-0">
        <div className="flex flex-wrap items-end gap-2">
          <div className="min-w-[120px]">
            <select value={kind} onChange={(e) => setKind(e.target.value)} className={INPUT_CLS}>
              {ITEM_KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
            </select>
          </div>
          <div className="min-w-[220px] flex-1">
            <input
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') void add(); }}
              placeholder="file something — a thought, a todo, a link…"
              className={INPUT_CLS}
            />
          </div>
          <Button variant="primary" icon={<Plus size={12} />} onClick={() => void add()}>add</Button>
        </div>
      </Card>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {items.length === 0 && (
          <p className="m-0 font-mono text-[11px] text-muted">
            nothing saved yet — file items here, or send them to a bot (it can be set to capture free text).
          </p>
        )}
        <div className="grid gap-1.5">
          {items.map((it) => (
            <div key={it.id} className="flex items-center gap-2.5 rounded-lg border border-border bg-surface px-2.5 py-1.5">
              <button
                className={`focus-ring grid h-4 w-4 shrink-0 cursor-pointer place-items-center rounded border font-mono text-[9px] ${it.status === 'done' ? 'border-foreground/40 bg-foreground/10 text-foreground' : 'border-border text-transparent hover:border-foreground/30'}`}
                onClick={() => void itemToggle(it.id).then(load)}
                aria-label={it.status === 'done' ? 'mark active' : 'mark done'}
              >✓</button>
              <span className="shrink-0 rounded border border-border px-1.5 py-0.5 font-mono text-[9px] tracking-wider text-muted uppercase">{it.kind}</span>
              <span className={`min-w-0 flex-1 truncate font-mono text-[11px] ${it.status === 'done' ? 'text-muted line-through' : 'text-foreground'}`}>{it.title || it.body}</span>
              {it.url && <a href={it.url} target="_blank" rel="noreferrer" className="shrink-0 font-mono text-[10px] text-muted hover:text-foreground">link</a>}
              <IconButton label="delete" onClick={() => void itemDelete(it.id).then(load)}><Trash2 size={11} /></IconButton>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
