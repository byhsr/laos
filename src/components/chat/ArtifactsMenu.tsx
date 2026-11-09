import { useEffect, useState } from 'react';
import { Copy, Download, FileText, Trash2 } from 'lucide-react';
import { ContextMenu, type MenuItem } from '../ui/ContextMenu';
import { IconButton } from '../ui/Button';
import { Markdown } from '../ui/Markdown';
import { SidePanel } from '../ui/SidePanel';
import { toast } from '../../hooks/useToast';
import { deleteDocument, listDocuments, onDocumentChanged, type ChatDocument } from '../../runtime';

// The artifacts an agent has written in this chat, behind one control in the chat
// header: a dropdown lists them all, and one opens in the side panel to read, copy
// or download. They never sit inline in the conversation.
const EXT: Record<string, string> = { html: 'html', text: 'txt', markdown: 'md' };
const MIME: Record<string, string> = { html: 'text/html', text: 'text/plain', markdown: 'text/markdown' };
const slug = (s: string) => s.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 60) || 'document';
const fmtWhen = (iso: string) => { const d = new Date(iso); return isNaN(d.getTime()) ? '' : d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }); };

export function ArtifactsMenu({ agentId, sessionId }: { agentId: string; sessionId: string | null }) {
  const [docs, setDocs] = useState<ChatDocument[]>([]);
  const [viewing, setViewing] = useState<ChatDocument | null>(null);

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

  // Keep the open viewer in step with the list — a deleted artifact closes it.
  useEffect(() => {
    if (viewing && !docs.some((d) => d.id === viewing.id)) setViewing(null);
  }, [docs, viewing]);

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

  const items: MenuItem[] = docs.length === 0
    ? [{ key: 'none', label: 'no artifacts yet', disabled: true, onSelect: () => {} }]
    : docs.map((d) => ({ key: d.id, label: d.title || 'untitled', icon: <FileText size={13} />, onSelect: () => setViewing(d) }));

  return (
    <>
      <ContextMenu
        items={items}
        title={docs.length ? `artifacts · ${docs.length}` : 'artifacts'}
        trigger={<FileText size={14} />}
        triggerClassName={`grid h-[26px] w-[26px] place-items-center rounded-lg transition-colors ${docs.length ? 'text-foreground' : 'text-muted'} hover:bg-background`}
      />

      {viewing && (
        <SidePanel
          title={viewing.title || 'artifact'}
          onClose={() => setViewing(null)}
          headerAction={
            <div className="flex items-center gap-1">
              <IconButton label="copy" onClick={() => void copy(viewing)}><Copy size={13} /></IconButton>
              <IconButton label="download" onClick={() => download(viewing)}><Download size={13} /></IconButton>
              <IconButton label="delete" onClick={() => void remove(viewing)}><Trash2 size={13} /></IconButton>
            </div>
          }
        >
          <div className="mb-2 font-mono text-[10px] tracking-wider text-muted uppercase">{viewing.kind} · {fmtWhen(viewing.createdAt)}</div>
          {viewing.kind === 'markdown'
            ? <Markdown>{viewing.content}</Markdown>
            : <pre className="m-0 whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-foreground/80">{viewing.content}</pre>}
        </SidePanel>
      )}
    </>
  );
}
