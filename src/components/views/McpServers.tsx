import { useEffect, useState } from 'react';
import { Plus, RefreshCw, Server, Trash2, Upload } from 'lucide-react';
import { deleteMcpServer, importMcpTools, listMcpServers, saveMcpServer, testMcpServer, type McpServer, type McpToolInfo } from '../../runtime';
import { toast } from '../../hooks/useToast';
import { useToolsStore } from '../../hooks/useTools';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS } from '../ui/Input';

type Draft = { id: string; name: string; command: string; args: string; env: string; enabled: boolean };

const emptyDraft = (): Draft => ({ id: '', name: '', command: '', args: '', env: '', enabled: true });

// "KEY=VALUE" per line <-> the env object the backend stores.
const parseEnvLines = (text: string): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const t = line.trim();
    if (!t || t.startsWith('#')) continue;
    const i = t.indexOf('=');
    if (i <= 0) continue;
    out[t.slice(0, i).trim()] = t.slice(i + 1).trim();
  }
  return out;
};
const envToLines = (env: Record<string, string>) =>
  Object.entries(env ?? {}).map(([k, v]) => `${k}=${v}`).join('\n');

// MCP connectors: point at a local MCP server, test it, then import its tools.
export function McpServers() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [found, setFound] = useState<Record<string, McpToolInfo[]>>({});
  // Last failure per server, kept inline so a long stderr tail is readable.
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);

  const load = async () => setServers(await listMcpServers());
  useEffect(() => { load(); }, []);

  const save = async () => {
    if (!draft) return;
    if (!draft.command.trim()) { toast('an MCP server needs a command to run', 'error'); return; }
    setSaving(true);
    try {
      const id = await saveMcpServer({
        id: draft.id,
        name: draft.name,
        command: draft.command.trim(),
        args: draft.args.split(/\s+/).filter(Boolean),
        env: parseEnvLines(draft.env),
        enabled: draft.enabled,
      });
      toast('mcp server saved', 'success');
      setDraft(null);
      await load();
      // Register the server's tools straight away so they show up in every
      // agent's Tools picker without a separate import click.
      await syncTools(id);
    } catch (e) {
      toast(typeof e === 'string' ? e : 'could not save the MCP server', 'error');
    } finally {
      setSaving(false);
    }
  };

  const test = async (id: string) => {
    setBusy(id);
    try {
      const tools = await testMcpServer(id);
      setFound((f) => ({ ...f, [id]: tools }));
      setErrors((e) => ({ ...e, [id]: '' }));
      toast(`server responded with ${tools.length} tool${tools.length === 1 ? '' : 's'}`, 'success');
    } catch (e) {
      const msg = typeof e === 'string' ? e : 'the MCP server did not respond';
      setFound((f) => ({ ...f, [id]: [] }));
      setErrors((err) => ({ ...err, [id]: msg }));
      toast(msg, 'error');
    } finally {
      setBusy(null);
    }
  };

  // Imports (or re-syncs) a server's advertised tools and refreshes the registry
  // so they appear in agent Tool pickers immediately.
  const syncTools = async (id: string) => {
    setBusy(id);
    try {
      const n = await importMcpTools(id);
      await useToolsStore.getState().loadTools();
      setErrors((e) => ({ ...e, [id]: '' }));
      toast(`imported ${n} tool${n === 1 ? '' : 's'} — attach them in an agent's tools list`, 'success');
    } catch (e) {
      const msg = typeof e === 'string' ? e : 'Import failed';
      setErrors((err) => ({ ...err, [id]: msg }));
      toast(msg, 'error');
    } finally {
      setBusy(null);
    }
  };

  const remove = async (id: string) => {
    await deleteMcpServer(id);
    await useToolsStore.getState().loadTools();
    toast('mcp server deleted', 'success');
    await load();
  };

  const label = `${FIELD_LABEL_CLS} mt-4`;

  return (
    <div className="mt-8">
      <div className="mb-2.5 flex items-center justify-between gap-3">
        <span className={GROUP_LABEL_CLS}>mcp servers</span>
        <Button variant="primary" icon={<Plus size={13} />} onClick={() => setDraft(emptyDraft())}>add server</Button>
      </div>

      {servers.length === 0 ? (
        <div className="grid place-items-center rounded-xl border border-dashed border-border p-6 text-center text-muted">
          <Server size={20} className="mb-2 opacity-50" />
          <p className="m-0 text-[12px]">No MCP servers yet. Add one, test it, then import its tools.</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-2.5 md:grid-cols-2 xl:grid-cols-3">
          {servers.map((s) => (
            <Card key={s.id} className="flex flex-col gap-3 hover:bg-surface">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <b className="block truncate font-mono text-[11px] text-foreground">{s.name || s.id}</b>
                  <span className="mt-0.5 block truncate font-mono text-[10px] text-muted">{s.command} {(s.args ?? []).join(' ')}</span>
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  <IconButton label="test connection" onClick={() => test(s.id)} disabled={busy === s.id}>
                    <RefreshCw size={12} className={busy === s.id ? 'animate-spin' : ''} />
                  </IconButton>
                  <IconButton label="sync tools" onClick={() => syncTools(s.id)} disabled={busy === s.id}><Upload size={12} /></IconButton>
                  <IconButton label="configure" onClick={() => setDraft({ id: s.id, name: s.name, command: s.command, args: (s.args ?? []).join(' '), env: envToLines(s.env), enabled: s.enabled })}><Server size={12} /></IconButton>
                  <IconButton label="delete" onClick={() => remove(s.id)}><Trash2 size={12} /></IconButton>
                </div>
              </div>

              {found[s.id] && (
                <div className="flex flex-wrap gap-1.5">
                  {found[s.id].length === 0
                    ? <span className="font-mono text-[10px] text-danger">no tools returned</span>
                    : found[s.id].map((t) => (
                      <span key={t.name} className="rounded bg-background px-2 py-0.5 font-mono text-[10px] text-muted">{t.name}</span>
                    ))}
                </div>
              )}

              {errors[s.id] && (
                <pre className="m-0 max-h-[200px] overflow-x-hidden overflow-y-auto rounded border border-danger/30 bg-background p-2.5 font-mono text-[10px] leading-relaxed break-words whitespace-pre-wrap text-danger">{errors[s.id]}</pre>
              )}
            </Card>
          ))}
        </div>
      )}

      {draft && (
        <Modal
          title={draft.id ? `${draft.name || draft.id} — mcp server` : 'new mcp server'}
          onClose={() => setDraft(null)}
          headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
        >
          <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
            Runs the server's command locally and speaks MCP over stdio. Secrets belong in env. Saving imports its tools automatically.
          </p>

          <label className={FIELD_LABEL_CLS}>name</label>
          <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} placeholder="Notion" className={INPUT_CLS} />

          <label className={label}>command</label>
          <input value={draft.command} onChange={(e) => setDraft({ ...draft, command: e.target.value })} placeholder="npx" className={INPUT_CLS} />

          <label className={label}>args</label>
          <input value={draft.args} onChange={(e) => setDraft({ ...draft, args: e.target.value })} placeholder="-y @notionhq/notion-mcp-server" className={INPUT_CLS} />
          <p className="mt-1.5 mb-0 font-mono text-[10px] text-muted">Space-separated. Quoted arguments aren't supported yet.</p>

          <label className={label}>env</label>
          <textarea value={draft.env} onChange={(e) => setDraft({ ...draft, env: e.target.value })} rows={3} placeholder={'NOTION_TOKEN=ntn_…'} className={`${INPUT_CLS} resize-none`} />
          <p className="mt-1.5 mb-0 font-mono text-[10px] text-muted">One KEY=VALUE per line. Values are stored locally and never shown again.</p>
        </Modal>
      )}
    </div>
  );
}
