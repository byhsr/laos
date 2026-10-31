import { useEffect, useState } from 'react';
import { BookOpen, Check, Search, Sparkles, Trash2 } from 'lucide-react';
import { deleteKnowledgeDoc, listKnowledgeDocs, refileKnowledgeApply, refileKnowledgePreview, saveKnowledgeDoc, type KnowledgeDoc, type RefileProposal } from '../../runtime';
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
  // The re-file pass: docs that are really memories or todos, awaiting approval.
  const [proposals, setProposals] = useState<RefileProposal[] | null>(null);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [tidying, setTidying] = useState(false);

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

  // Proposes where each stray doc should live; nothing moves until approved.
  const openTidy = async () => {
    setTidying(true);
    const found = await refileKnowledgePreview();
    setTidying(false);
    if (found.length === 0) { toast('knowledge base looks clean — nothing to move', 'success'); return; }
    setChosen(new Set(found.map((p) => p.docId)));
    setProposals(found);
  };

  const applyTidy = async () => {
    if (!proposals) return;
    const selected = proposals.filter((p) => chosen.has(p.docId));
    if (selected.length === 0) { setProposals(null); return; }
    setTidying(true);
    try {
      const moved = await refileKnowledgeApply(selected);
      toast(`moved ${moved} doc${moved === 1 ? '' : 's'}`, 'success');
      setProposals(null);
      await load();
    } finally {
      setTidying(false);
    }
  };

  const toggleChosen = (docId: string) =>
    setChosen((prev) => {
      const next = new Set(prev);
      if (next.has(docId)) next.delete(docId); else next.add(docId);
      return next;
    });

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
        <Button className="shrink-0" icon={<Sparkles size={13} />} onClick={() => void openTidy()} disabled={tidying}>tidy</Button>
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

      {proposals && (
        <Modal
          title="tidy knowledge base"
          onClose={() => setProposals(null)}
          width="min(92vw, 640px)"
          headerAction={
            <div className="flex items-center gap-1.5">
              <Button onClick={() => setProposals(null)}>cancel</Button>
              <Button variant="primary" icon={<Check size={13} />} onClick={() => void applyTidy()} disabled={tidying || chosen.size === 0}>
                {tidying ? 'moving…' : `move ${chosen.size}`}
              </Button>
            </div>
          }
        >
          <p className="mt-0 mb-3 text-[12px] leading-relaxed text-muted">
            These docs look like memories or todos, not shared reference material. Rules and facts about you move to
            memory (global); things to do move to items. Nothing is changed until you approve.
          </p>
          <div className="grid gap-2">
            {proposals.map((p) => (
              <button
                key={p.docId}
                type="button"
                className="flex w-full cursor-pointer items-start gap-2.5 rounded-lg border border-border bg-surface p-3 text-left transition-colors duration-150 hover:bg-background"
                onClick={() => toggleChosen(p.docId)}
              >
                <span className={`mt-0.5 grid h-4 w-4 shrink-0 place-items-center rounded border ${chosen.has(p.docId) ? 'border-foreground bg-foreground text-background' : 'border-border'}`}>
                  {chosen.has(p.docId) && <Check size={11} />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-2">
                    <b className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{p.title || 'untitled'}</b>
                    <span className="shrink-0 rounded bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted">
                      {p.destination === 'memory' ? `memory · ${p.kind}` : `items · ${p.kind}`}
                    </span>
                  </span>
                  <span className="mt-1 block line-clamp-2 text-[11px] leading-relaxed whitespace-pre-wrap text-muted">
                    {p.content.slice(0, 160) || 'empty doc'}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </Modal>
      )}
    </>
  );
}
