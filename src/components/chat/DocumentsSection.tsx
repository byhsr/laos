import { useEffect, useState } from 'react';
import { ChevronRight, Copy, Download, FileText, Trash2 } from 'lucide-react';
import { Markdown } from '../ui/Markdown';
import { IconButton } from '../ui/Button';
import { Tooltip } from '../ui/Tooltip';
import { toast } from '../../hooks/useToast';
import { deleteDocument, listDocuments, onDocumentChanged, type ChatDocument } from '../../runtime';

// The artifacts an agent has written in this chat: titled markdown/text/html files
// it can produce with the `document` tool, shown inline so the user can read, copy
// and download them instead of scrolling a wall of text in the reply.
const EXT: Record<string, string> = { html: 'html', text: 'txt', markdown: 'md' };
const MIME: Record<string, string> = { html: 'text/html', text: 'text/plain', markdown: 'text/markdown' };
const slug = (s: string) => s.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 60) || 'document';
const fmtWhen = (iso: string) => { const d = new Date(iso); return isNaN(d.getTime()) ? '' : d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }); };

export function DocumentsSection({ agentId, sessionId }: { agentId: string; sessionId: string | null }) {
  const [docs, setDocs] = useState<ChatDocument[]>([]);
  const [open, setOpen] = useState<Set<string>>(new Set());

  // Load this chat's documents, and refresh whenever one is written or removed.
  useEffect(() => {
    const sid = sessionId;
    if (!sid) { setDocs([]); return; }
    let alive = true;
    const load = () => { void listDocuments(agentId, sid).then((d) => { if (alive) setDocs(d); }); };
    load();
    const off = onDocumentChanged((e) => { if (e.agentId === agentId && e.sessionId === sid) load(); });
    return () => { alive = false; off(); };
  }, [agentId, sessionId]);

  const download = (d: ChatDocument) => {
    const blob = new Blob([d.content], { type: MIME[d.kind] ?? 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${slug(d.title)}.${EXT[d.kind] ?? 'md'}`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const copy = async (d: ChatDocument) => {
    try { await navigator.clipboard.writeText(d.content); toast('copied', 'success'); }
    catch { toast('could not copy', 'error'); }
  };

  const remove = async (d: ChatDocument) => {
    try { await deleteDocument(d.id); setDocs((prev) => prev.filter((x) => x.id !== d.id)); }
    catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };

  const toggle = (id: string) => setOpen((prev) => { const n = new Set(prev); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  if (docs.length === 0) return null;

  return (
    <div className="mt-4 flex flex-col gap-2">
      <span className="font-mono text-[10px] tracking-wider text-muted uppercase">artifacts · {docs.length}</span>
      {docs.map((d) => {
        const isOpen = open.has(d.id);
        return (
          <div key={d.id} className="overflow-hidden rounded-xl border border-border bg-surface">
            <div className="flex items-center gap-2 px-3 py-2">
              <FileText size={13} className="shrink-0 text-muted" />
              <button
                type="button"
                className="focus-ring flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 border-0 bg-transparent p-0 text-left"
                onClick={() => toggle(d.id)}
                aria-expanded={isOpen}
              >
                <ChevronRight size={11} className={`shrink-0 text-muted transition-transform duration-150 ${isOpen ? 'rotate-90' : ''}`} />
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{d.title}</span>
              </button>
              <span className="shrink-0 rounded border border-border px-1.5 py-0.5 font-mono text-[9px] tracking-wider text-muted uppercase">{d.kind}</span>
              <span className="shrink-0 font-mono text-[10px] text-muted/70">{fmtWhen(d.createdAt)}</span>
              <Tooltip label="copy"><IconButton label="copy" className="h-6 w-6" onClick={() => void copy(d)}><Copy size={11} /></IconButton></Tooltip>
              <Tooltip label="download"><IconButton label="download" className="h-6 w-6" onClick={() => download(d)}><Download size={11} /></IconButton></Tooltip>
              <IconButton label="delete" className="h-6 w-6" onClick={() => void remove(d)}><Trash2 size={11} /></IconButton>
            </div>
            {isOpen && (
              <div className="border-t border-border px-3.5 py-3">
                {d.kind === 'markdown'
                  ? <Markdown>{d.content}</Markdown>
                  : <pre className="m-0 max-h-[440px] overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-foreground/80">{d.content}</pre>}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
