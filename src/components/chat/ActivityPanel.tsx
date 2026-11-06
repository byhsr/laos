import { useEffect, useRef, useState } from 'react';
import { Check, ChevronRight, FileText, GripVertical, Square, TriangleAlert, X } from 'lucide-react';
import { useActivity, type ActivityItem } from '../../hooks/useActivity';
import { cancelTask } from '../../runtime';
import { Button } from '../ui/Button';
import { tabCls } from '../ui/tabs';

const secs = (ms: number) => (Number.isFinite(ms) && ms >= 0 ? `${(ms / 1000).toFixed(1)}s` : '');
const chars = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k chars` : `${n} chars`);
// Past this, a result stops being a footnote and becomes a document: the card
// keeps a chip instead of dumping the whole thing.
const LONG_RESULT = 400;

// The chat's right-hand split: a resizable stack of live jobs — the current turn
// and every background task — so what's working is visible while the chat stays
// put. Cards are drag-reorderable; a job that ends fires a short burst. A result
// too long for its card becomes a chip that opens the editable `text` view.
export function ActivityPanel({ agentId, onStopTurn }: { agentId: string; onStopTurn?: () => void }) {
  const items = useActivity((s) => s.items);
  const width = useActivity((s) => s.width);
  const setWidth = useActivity((s) => s.setWidth);
  const setOpen = useActivity((s) => s.setOpen);
  const reorder = useActivity((s) => s.reorder);
  const remove = useActivity((s) => s.remove);
  const editingId = useActivity((s) => s.editing);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [tab, setTab] = useState<'activity' | 'text'>('activity');
  const dragId = useRef<string | null>(null);

  // Opening a chip jumps straight to the text view.
  useEffect(() => { if (editingId) setTab('text'); }, [editingId]);

  // Dragging the left edge left grows the panel (it is docked right).
  const onResizeStart = (e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    const move = (ev: PointerEvent) => setWidth(startW + (startX - ev.clientX));
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      document.body.style.cursor = '';
    };
    document.body.style.cursor = 'col-resize';
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  // A delegated task shows in its requesting chat and in the assigned agent's
  // panel — so opening the window the work runs in shows the job, not nothing.
  const mine = items.filter((i) => i.agentId === agentId || i.assignedAgent === agentId);
  const running = mine.filter((i) => i.status === 'running').length;
  const editingItem = tab === 'text' ? (mine.find((i) => i.id === editingId) ?? null) : null;

  const toggleItem = (id: string) => setExpanded((prev) => {
    const next = new Set(prev);
    if (next.has(id)) next.delete(id); else next.add(id);
    return next;
  });

  return (
    <div className="app-no-drag relative flex h-full min-h-0 shrink-0 flex-col border-l border-border" style={{ width }}>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="resize activity panel"
        onPointerDown={onResizeStart}
        className="absolute inset-y-0 left-0 z-10 w-1 cursor-col-resize bg-transparent transition-colors hover:bg-foreground/20"
      />

      <div className="flex shrink-0 items-center gap-1 border-b border-border px-2 py-1.5">
        <button type="button" className={`${tabCls(!editingItem)} cursor-pointer`} onClick={() => setTab('activity')}>activity</button>
        {editingId && <button type="button" className={`${tabCls(!!editingItem)} cursor-pointer`} onClick={() => setTab('text')}>text</button>}
        <span className="ml-auto shrink-0 font-mono text-[10px] text-muted">{running > 0 ? `${running} running` : `${mine.length}`}</span>
        <button
          type="button"
          aria-label="close activity panel"
          onClick={() => setOpen(false)}
          className="focus-ring grid h-5 w-5 shrink-0 cursor-pointer place-items-center rounded border-0 bg-transparent p-0 text-muted hover:text-foreground"
        >
          <X size={12} />
        </button>
      </div>

      {editingItem ? (
        <TextEditor item={editingItem} />
      ) : (
        <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto p-2">
          {mine.length === 0 ? (
            <p className="m-0 px-1 py-3 font-mono text-[10px] leading-relaxed text-muted">
              nothing running — a delegated task or a live turn shows up here.
            </p>
          ) : (
            <div className="flex flex-col gap-1.5">
              {mine.map((it) => (
                <ActivityCard
                  key={it.id}
                  item={it}
                  expanded={expanded.has(it.id)}
                  onToggle={() => toggleItem(it.id)}
                  onRemove={() => remove(it.id)}
                  onEdit={() => useActivity.getState().beginEdit(it.id)}
                  onStop={it.kind === 'task' ? () => { void cancelTask(it.id); } : onStopTurn}
                  onDragStart={() => { dragId.current = it.id; }}
                  onDropOn={() => { if (dragId.current && dragId.current !== it.id) reorder(dragId.current, it.id); dragId.current = null; }}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// The `text` view: the job's result as an editable document, written back to the
// task row on save.
function TextEditor({ item }: { item: ActivityItem }) {
  const text = useActivity((s) => s.editText);
  const setText = useActivity((s) => s.setEditText);
  const save = useActivity((s) => s.saveEdit);
  const reset = useActivity((s) => s.resetEdit);
  const dirty = text !== (item.result ?? '');

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-1.5">
        <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-muted">{item.title}</span>
        <span className="shrink-0 font-mono text-[10px] text-muted/60">{chars(text.length)}</span>
      </div>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        spellCheck={false}
        aria-label="edit result"
        className="min-h-0 flex-1 resize-none border-0 bg-transparent p-3 font-mono text-[11px] leading-relaxed text-foreground outline-none"
      />
      <div className="flex shrink-0 items-center gap-1.5 border-t border-border px-3 py-2">
        <Button variant="primary" disabled={!dirty} onClick={save}>save</Button>
        <Button disabled={!dirty} onClick={reset}>revert</Button>
        <span className="ml-auto font-mono text-[10px] text-muted">{dirty ? 'unsaved' : 'saved'}</span>
      </div>
    </div>
  );
}

function ActivityCard({ item, expanded, onToggle, onRemove, onEdit, onStop, onDragStart, onDropOn }: {
  item: ActivityItem;
  expanded: boolean;
  onToggle: () => void;
  onRemove: () => void;
  onEdit: () => void;
  onStop?: () => void;
  onDragStart: () => void;
  onDropOn: () => void;
}) {
  const [now, setNow] = useState(() => Date.now());
  const [burst, setBurst] = useState(false);
  const running = item.status === 'running';
  const long = !!item.result && item.result.length > LONG_RESULT;

  // Tick the elapsed seconds only while the job is live.
  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(t);
  }, [running]);

  // Fire the burst each time the job ends (burst counter bumps on finish).
  useEffect(() => {
    if (!item.burst) return;
    setBurst(true);
    const t = setTimeout(() => setBurst(false), 900);
    return () => clearTimeout(t);
  }, [item.burst]);

  // Lead with the latest concrete step, matching the chat bar — a `think`
  // placeholder (planning / raw reasoning) only shows when it is all there is.
  const last = [...item.steps].reverse().find((s) => s.kind !== 'think') ?? item.steps[item.steps.length - 1];

  return (
    <div
      draggable
      onDragStart={onDragStart}
      onDragOver={(e) => e.preventDefault()}
      onDrop={onDropOn}
      className={`group rounded-lg border border-border bg-surface px-2.5 py-2 transition-colors ${burst ? 'activity-burst' : ''}`}
    >
      <div className="flex items-center gap-2">
        <span title="drag to reorder" aria-hidden className="shrink-0 cursor-grab text-muted/40 group-hover:text-muted">
          <GripVertical size={12} />
        </span>
        <span
          className={`h-1.5 w-1.5 shrink-0 rounded-full ${running ? 'animate-pulse bg-foreground/60' : item.status === 'failed' ? 'bg-danger' : 'bg-muted'}`}
          title={item.status}
        />
        <button
          type="button"
          onClick={onToggle}
          className="focus-ring flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 border-0 bg-transparent p-0 text-left"
        >
          <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{item.title}</span>
          <span className="shrink-0 font-mono text-[10px] text-muted/60">{secs((item.endedAt ?? now) - item.startedAt)}</span>
          <ChevronRight size={11} className={`shrink-0 text-muted transition-transform ${expanded ? 'rotate-90' : ''}`} />
        </button>
        {/* A running job can be stopped from here — a background task is
            cancelled outright, a live turn is halted. */}
        {running && onStop && (
          <button
            type="button"
            aria-label="stop"
            onClick={onStop}
            className="focus-ring grid h-5 w-5 shrink-0 cursor-pointer place-items-center rounded border-0 bg-transparent p-0 text-muted transition-colors hover:text-danger"
          >
            <Square size={10} />
          </button>
        )}
        {!running && (
          <button
            type="button"
            aria-label="dismiss"
            onClick={onRemove}
            className="focus-ring shrink-0 cursor-pointer border-0 bg-transparent p-0 text-muted/50 hover:text-foreground"
          >
            <X size={11} />
          </button>
        )}
      </div>

      {running && last && <p className="mt-1 mb-0 truncate pl-6 font-mono text-[10px] text-muted">{last.text}</p>}

      {/* Extra text — too much for the card, so it becomes a chip into the editor. */}
      {long && (
        <button
          type="button"
          onClick={onEdit}
          className="focus-ring mt-1.5 ml-6 inline-flex cursor-pointer items-center gap-1 rounded border border-border bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted hover:text-foreground"
        >
          <FileText size={10} /> result · {chars(item.result!.length)}
        </button>
      )}

      {expanded && (
        <div className="mt-2 border-l border-border pl-3">
          {item.detail && <p className="m-0 mb-1.5 font-mono text-[10px] text-muted">{item.detail}</p>}
          {item.steps.length > 0 && (
            <ol className="m-0 flex list-none flex-col gap-1 p-0">
              {item.steps.map((s, i) => (
                <li key={i} className="flex min-w-0 gap-2 font-mono text-[10px] leading-relaxed">
                  <span className="w-9 shrink-0 tracking-wider text-muted uppercase">{s.kind}</span>
                  <span className="min-w-0 whitespace-pre-wrap break-words text-foreground/70">{s.text}</span>
                </li>
              ))}
            </ol>
          )}
          {item.result && !long && (
            <pre className="mt-1.5 mb-0 max-h-[220px] overflow-auto whitespace-pre-wrap break-words font-mono text-[10px] text-muted">{item.result}</pre>
          )}
          {item.status === 'done' && (
            <p className="m-0 mt-1.5 flex items-center gap-1 font-mono text-[10px] text-muted"><Check size={10} /> finished</p>
          )}
          {item.status === 'failed' && (
            <p className="m-0 mt-1.5 flex items-center gap-1 font-mono text-[10px] text-danger"><TriangleAlert size={10} /> failed</p>
          )}
        </div>
      )}
    </div>
  );
}
