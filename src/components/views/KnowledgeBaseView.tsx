import { useEffect, useState } from 'react';
import { BookOpen, Check, Plus, Search, Trash2, X } from 'lucide-react';
import { deleteKnowledgeDoc, listKnowledgeDocs, saveKnowledgeDoc, type KnowledgeDoc } from '../../runtime';
import { toast } from '../../hooks/useToast';

const fmtDate = (s?: string) => {
  if (!s) return '';
  const d = new Date(s);
  return isNaN(d.getTime()) ? '' : d.toLocaleString();
};

export function KnowledgeBaseView({ embedded = false }: { embedded?: boolean }) {
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
    if (!editing || !draft.title.trim()) { toast('Document needs a title', 'error'); return; }
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
      toast('Document saved', 'success');
      setEditing(null);
      await load();
    } finally {
      setSaving(false);
    }
  };

  const remove = async (doc: KnowledgeDoc) => {
    await deleteKnowledgeDoc(doc.id);
    toast('Document deleted', 'success');
    await load();
  };

  const q = search.trim().toLowerCase();
  const filtered = q
    ? docs.filter((d) => d.title.toLowerCase().includes(q) || d.content.toLowerCase().includes(q) || d.tags.some((t) => t.toLowerCase().includes(q)))
    : docs;

  return (
    <>
      <header className="mb-6 flex items-end justify-between">
        {!embedded && (
          <div>
            <span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span>
            <h1 style={{ margin: 0, fontSize: 24 }}>Knowledge Base</h1>
          </div>
        )}
        <button className="primary ml-auto" onClick={newDoc}><Plus size={14} />New doc</button>
      </header>

      <div className="mb-4 flex items-center gap-2">
        <div className="relative flex-1 max-w-[420px]">
          <Search size={13} className="absolute top-1/2 left-3 -translate-y-1/2 text-muted" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search docs…"
            className="w-full rounded-md border border-line bg-panel2 py-2 pr-3 pl-9 text-[12.5px] text-text outline-none placeholder:text-muted focus:border-mid"
          />
        </div>
        <span className="text-[11px] text-muted">{filtered.length} doc{filtered.length === 1 ? '' : 's'}</span>
      </div>

      {loading ? (
        <p className="text-[12px] text-muted">Loading…</p>
      ) : filtered.length === 0 ? (
        <div className="grid min-h-[280px] place-items-center rounded-[12px] border border-dashed border-soft">
          <div className="text-center text-muted">
            <BookOpen size={28} className="mx-auto mb-2 opacity-50" />
            <p className="text-[12px]">No docs yet. Create one — or ask Laos to save company wiki, ICP notes, and decisions here.</p>
          </div>
        </div>
      ) : (
        <div className="grid max-w-[1100px] grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {filtered.map((d) => (
            <div key={d.id} className="group flex cursor-pointer flex-col rounded-[10px] border border-line bg-panel p-4 text-left transition-all duration-150 hover:-translate-y-0.5 hover:border-dotted hover:border-mid hover:shadow-[0_8px_24px_#0005]" onClick={() => openDoc(d)}>
              <div className="flex items-start justify-between gap-2">
                <b className="block truncate text-[13px]">{d.title || 'Untitled'}</b>
                <button className="cursor-pointer border-0 bg-transparent p-0 text-muted opacity-0 transition-opacity hover:text-[#f87171] group-hover:opacity-100" onClick={(e) => { e.stopPropagation(); remove(d); }} title="Delete doc"><Trash2 size={13} /></button>
              </div>
              <p className="mt-2 line-clamp-3 min-h-[45px] text-[11px] leading-[1.5] whitespace-pre-wrap text-muted">{d.content || 'Empty doc'}</p>
              {d.tags.length > 0 && (
                <div className="mt-3 flex flex-wrap gap-1">
                  {d.tags.map((t) => <span key={t} className="rounded bg-panel2 px-1.5 py-0.5 font-mono text-[9px] text-muted">{t}</span>)}
                </div>
              )}
              <span className="mt-3 block font-mono text-[9px] text-muted">updated {fmtDate(d.updatedAt)}</span>
            </div>
          ))}
        </div>
      )}

      {editing && (
        <div className="fixed inset-0 z-[90] grid place-items-center bg-black/60" onClick={() => setEditing(null)}>
          <div className="flex max-h-[85vh] w-[680px] max-w-[94vw] flex-col rounded-[12px] border border-line bg-panel shadow-[0_20px_60px_#000a]" onClick={(e) => e.stopPropagation()}>
            <div className="flex h-[52px] flex-none items-center justify-between border-b border-line px-4">
              <b className="text-[13px]">{draft.title ? `Edit: ${draft.title}` : 'New document'}</b>
              <button className="secondary" onClick={() => setEditing(null)}><X size={13} />Close</button>
            </div>
            <div className="flex-1 overflow-y-auto p-4">
              <label className="mb-1 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">TITLE</label>
              <input value={draft.title} onChange={(e) => setDraft({ ...draft, title: e.target.value })} placeholder="e.g. ICP — Ideal Customer Profile" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[13px] text-text outline-none focus:border-mid" />

              <label className="mt-4 mb-1 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">CONTENT</label>
              <textarea value={draft.content} onChange={(e) => setDraft({ ...draft, content: e.target.value })} rows={14} placeholder="Markdown supported. Anything agents and you should know…" className="w-full resize-y rounded-md border border-line bg-panel2 px-3 py-2 font-mono text-[12px] leading-[1.7] text-text outline-none focus:border-mid" />

              <label className="mt-4 mb-1 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">TAGS</label>
              <input value={draft.tags} onChange={(e) => setDraft({ ...draft, tags: e.target.value })} placeholder="icp, wiki, decisions (comma separated)" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid" />
            </div>
            <div className="flex h-[56px] flex-none items-center justify-end gap-2 border-t border-line px-4">
              <button className="secondary" onClick={() => setEditing(null)}>Cancel</button>
              <button className="primary" onClick={save} disabled={saving}><Check size={13} />{saving ? 'Saving…' : 'Save doc'}</button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
