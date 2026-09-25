import { useEffect, useState } from 'react';
import { BookOpen, Check, Search, Trash2 } from 'lucide-react';
import { deleteKnowledgeDoc, listKnowledgeDocs, saveKnowledgeDoc, type KnowledgeDoc } from '../../runtime';
import { toast } from '../../hooks/useToast';
import { useAgentsStore } from '../../hooks/useAgents';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, INPUT_CLS, PROSE_CLS } from '../ui/Input';

const fmtDate = (s?: string) => {
  if (!s) return '';
  const d = new Date(s);
  return isNaN(d.getTime()) ? '' : d.toLocaleString();
};

export function KnowledgeBaseView() {
  // The lead agent's configured name, never a hardcoded one.
  const leadName = useAgentsStore((s) => s.agents.find((a) => a.isManager)?.name?.trim() || 'the lead agent');
  const [docs, setDocs] = useState<KnowledgeDoc[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [editing, setEditing] = useState<KnowledgeDoc | null>(null);
  const [draft, setDraft] = useState({ title: '', content: '', tags: '' });
  const [saving, setSaving] = useState(false);

  const load = async () => {
    setLoading(true);
    const d = await listKnowledgeDocs();
    setDocs(d);
    setLoading(false);
  };

  useEffect(() => { void load(); }, []);

  const newDoc = () => {
    setDraft({ title: '', content: '', tags: '' });
    setEditing({ id: `kb-${Date.now()}`, title: '', content: '', tags: [], updatedAt: new Date().toISOString() });
  };

  const openDoc = (doc: KnowledgeDoc) => {
    setEditing(doc);
    setDraft({ title: doc.title, content: doc.content, tags: doc.tags.join(', ') });
  };

  const save = async () => {
    if (!editing || !draft.title.trim()) { toast('document needs a title', 'error'); return; }
    setSaving(true);
    try {
      const doc: KnowledgeDoc = {
        ...editing,
        title: draft.title.trim(),
        content: draft.content,
        tags: draft.tags.split(',').map((t) => t.trim()).filter(Boolean),
        updatedAt: new Date().toISOString(),
      };
      await saveKnowledgeDoc(doc);
      toast('document saved', 'success');
      setEditing(null);
      await load();
    } finally {
      setSaving(false);
    }
  };

  const remove = async (doc: KnowledgeDoc) => {
    await deleteKnowledgeDoc(doc.id);
    toast('document deleted', 'success');
    await load();
  };

  const q = search.trim().toLowerCase();
  const filtered = q
    ? docs.filter((d) => d.title.toLowerCase().includes(q) || d.content.toLowerCase().includes(q) || d.tags.some((t) => t.toLowerCase().includes(q)))
    : docs;

  return (
    <>
      <div className="mb-4 flex items-center gap-3">
        <div className="relative min-w-0 flex-1">
          <Search size={13} className="absolute top-1/2 left-2.5 -translate-y-1/2 text-muted" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="search docs…"
            className={`${INPUT_CLS} py-1.5 pr-2.5 pl-8`}
          />
        </div>
        <span className="shrink-0 font-mono text-[10px] text-muted">{filtered.length} doc{filtered.length === 1 ? '' : 's'}</span>
        <Button variant="primary" className="shrink-0" icon={<Check size={13} />} onClick={newDoc}>new doc</Button>
      </div>

      {loading ? (
        <p className="m-0 font-mono text-[11px] text-muted">loading…</p>
      ) : filtered.length === 0 ? (
        <div className="grid min-h-[240px] place-items-center rounded-xl border border-dashed border-border">
          <div className="max-w-[420px] px-6 text-center text-muted">
            <BookOpen size={26} className="mx-auto mb-2 opacity-50" />
            <p className="m-0 text-[12px] leading-relaxed">No docs yet. Create one — or ask {leadName} to save company wiki, ICP notes, and decisions here.</p>
          </div>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-2.5 md:grid-cols-2 xl:grid-cols-3">
          {filtered.map((d) => (
            <div key={d.id} className="group flex cursor-pointer flex-col rounded-xl border border-border bg-surface p-3.5 text-left transition-colors duration-150 hover:bg-background" onClick={() => openDoc(d)}>
              <div className="flex items-start justify-between gap-2">
                <b className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{d.title || 'untitled'}</b>
                <span className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100">
                  <IconButton label="delete doc" className="h-6 w-6" onClick={(e) => { e.stopPropagation(); void remove(d); }}><Trash2 size={11} /></IconButton>
                </span>
              </div>
              <p className="mt-2 mb-0 line-clamp-3 min-h-[42px] text-[12px] leading-relaxed whitespace-pre-wrap text-muted">{d.content || 'empty doc'}</p>
              {d.tags.length > 0 && (
                <div className="mt-2.5 flex flex-wrap gap-1">
                  {d.tags.map((t) => <span key={t} className="rounded bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted">{t}</span>)}
                </div>
              )}
              <span className="mt-2.5 block font-mono text-[10px] text-muted">updated {fmtDate(d.updatedAt)}</span>
            </div>
          ))}
        </div>
      )}

      {editing && (
        <Modal
          title={draft.title ? `edit ${draft.title}` : 'new document'}
          onClose={() => setEditing(null)}
          width="min(92vw, 720px)"
          headerAction={
            <div className="flex items-center gap-1.5">
              <Button onClick={() => setEditing(null)}>cancel</Button>
              <Button variant="primary" icon={<Check size={13} />} onClick={save} disabled={saving}>{saving ? 'saving…' : 'save doc'}</Button>
            </div>
          }
        >
          <label className={FIELD_LABEL_CLS}>title</label>
          <input value={draft.title} onChange={(e) => setDraft({ ...draft, title: e.target.value })} placeholder="e.g. ICP — Ideal Customer Profile" className={INPUT_CLS} />

          <label className={`${FIELD_LABEL_CLS} mt-4`}>content</label>
          <textarea value={draft.content} onChange={(e) => setDraft({ ...draft, content: e.target.value })} rows={14} placeholder="Markdown supported. Anything agents and you should know…" className={PROSE_CLS} />

          <label className={`${FIELD_LABEL_CLS} mt-4`}>tags</label>
          <input value={draft.tags} onChange={(e) => setDraft({ ...draft, tags: e.target.value })} placeholder="icp, wiki, decisions (comma separated)" className={INPUT_CLS} />
        </Modal>
      )}
    </>
  );
}
