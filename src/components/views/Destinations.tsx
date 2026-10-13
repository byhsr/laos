import { useEffect, useState } from 'react';
import { Plus, Send, Trash2 } from 'lucide-react';
import {
  DEFAULT_ARG_TEMPLATE, deleteDestination, destinationTools, listDestinations, listMcpServers,
  saveDestination, testDestination, type Destination, type McpServer,
} from '../../runtime';
import { toast } from '../../hooks/useToast';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { Select } from '../ui/Select';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS } from '../ui/Input';

const empty = (): Destination => ({ id: '', name: '', serverId: '', tool: '', argTemplate: DEFAULT_ARG_TEMPLATE, enabled: true, createdAt: '', updatedAt: '' });

// Destinations: where captured items go. Each is an MCP server + one of its tools
// + a JSON arg template, so several apps (e.g. two Notion accounts, each its own
// server) can each receive ideas directly.
export function Destinations() {
  const [items, setItems] = useState<Destination[]>([]);
  const [servers, setServers] = useState<McpServer[]>([]);
  const [tools, setTools] = useState<string[]>([]);
  const [draft, setDraft] = useState<Destination | null>(null);
  const [saving, setSaving] = useState(false);

  const load = async () => { setItems(await listDestinations()); setServers(await listMcpServers()); };
  useEffect(() => { load(); }, []);
  useEffect(() => { if (draft?.serverId) destinationTools(draft.serverId).then(setTools); else setTools([]); }, [draft?.serverId]);

  const save = async () => {
    if (!draft) return;
    if (!draft.name.trim()) { toast('Give the destination a name', 'error'); return; }
    if (!draft.serverId || !draft.tool) { toast('Pick a server and a tool', 'error'); return; }
    setSaving(true);
    try {
      await saveDestination(draft);
      setDraft(null);
      await load();
      toast('destination saved', 'success');
    } catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
    finally { setSaving(false); }
  };

  const test = async (d: Destination) => {
    try { toast(await testDestination(d), 'success'); }
    catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };

  const label = `${FIELD_LABEL_CLS} mt-4`;

  return (
    <div className="mt-8">
      <div className="mb-2.5 flex items-center justify-between gap-3">
        <span className={GROUP_LABEL_CLS}>destinations</span>
        <Button variant="primary" icon={<Plus size={13} />} onClick={() => setDraft(empty())}>add destination</Button>
      </div>
      <p className="mt-0 mb-2.5 text-[11px] leading-[1.6] text-muted">
        where captured items go — an MCP server plus one of its tools. A capture bot sends to its destination automatically; any item can be sent from the <span className="font-mono">send to ▸</span> menu in Items.
      </p>

      {items.length === 0 ? (
        <div className="grid place-items-center rounded-xl border border-dashed border-border p-6 text-center text-muted">
          <Send size={20} className="mb-2 opacity-50" />
          <p className="m-0 text-[12px]">No destinations yet. Add one, then pick it in a bot's settings.</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-2.5 md:grid-cols-2 xl:grid-cols-3">
          {items.map((d) => (
            <Card key={d.id} className="flex flex-col gap-3 hover:bg-surface">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <b className="block truncate font-mono text-[11px] text-foreground">{d.name || d.tool}</b>
                  <span className="mt-0.5 block truncate font-mono text-[10px] text-muted">
                    {servers.find((s) => s.id === d.serverId)?.name ?? d.serverId} → {d.tool}{d.enabled ? '' : ' · off'}
                  </span>
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  <IconButton label="test" onClick={() => void test(d)}><Send size={12} /></IconButton>
                  <IconButton label="configure" onClick={() => setDraft(d)}><Plus size={12} /></IconButton>
                  <IconButton label="delete" onClick={() => void deleteDestination(d.id).then(load)}><Trash2 size={12} /></IconButton>
                </div>
              </div>
            </Card>
          ))}
        </div>
      )}

      {draft && (
        <Modal
          title="destination"
          onClose={() => setDraft(null)}
          headerAction={<Button variant="primary" disabled={saving} onClick={() => void save()}>{saving ? 'saving…' : 'save'}</Button>}
        >
          <label className={FIELD_LABEL_CLS}>name</label>
          <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} placeholder="Notion — personal" className={INPUT_CLS} />

          <label className={label}>server</label>
          <Select
            value={draft.serverId}
            options={servers.map((s) => ({ value: s.id, label: s.name || s.id }))}
            onChange={(v) => setDraft({ ...draft, serverId: v, tool: '' })}
            placeholder="pick an MCP server…"
          />

          <label className={label}>tool</label>
          {tools.length > 0 ? (
            <Select value={draft.tool} options={tools.map((t) => ({ value: t, label: t }))} onChange={(v) => setDraft({ ...draft, tool: v })} placeholder="pick a tool…" />
          ) : (
            <input value={draft.tool} onChange={(e) => setDraft({ ...draft, tool: e.target.value })} placeholder="import the server's tools first, or type a tool name" className={INPUT_CLS} />
          )}

          <label className={label}>arg template (JSON)</label>
          <textarea
            value={draft.argTemplate}
            onChange={(e) => setDraft({ ...draft, argTemplate: e.target.value })}
            rows={6}
            spellCheck={false}
            className={`${INPUT_CLS} font-mono text-[11px] leading-relaxed`}
          />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
            placeholders: <span className="text-foreground">{'{{title}} {{body}} {{url}} {{tags}} {{kind}}'}</span> — the item fills them, then the result must parse as JSON.
          </p>
        </Modal>
      )}
    </div>
  );
}
