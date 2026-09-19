import { useEffect, useState } from 'react';
import { Check, Plus, RefreshCw, Server, Trash2, Upload } from 'lucide-react';
import { deleteMcpServer, importMcpTools, listMcpServers, saveMcpServer, testMcpServer, type McpServer, type McpToolInfo } from '../../runtime';
import { toast } from '../../hooks/useToast';
import { useToolsStore } from '../../hooks/useTools';
import { Drawer } from '../ui/Drawer';

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
    if (!draft.command.trim()) { toast('An MCP server needs a command to run.', 'error'); return; }
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
      toast('MCP server saved', 'success');
      setDraft(null);
      await load();
      // Register the server's tools straight away so they show up in every
      // agent's Tools picker without a separate import click.
      await syncTools(id);
    } catch (e) {
      toast(typeof e === 'string' ? e : 'Could not save the MCP server', 'error');
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
      toast(`Server responded with ${tools.length} tool${tools.length === 1 ? '' : 's'}`, 'success');
    } catch (e) {
      const msg = typeof e === 'string' ? e : 'The MCP server did not respond';
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
      toast(`Imported ${n} tool${n === 1 ? '' : 's'} — attach them in an agent's Tools list`, 'success');
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
    toast('MCP server deleted', 'success');
    await load();
  };

  return (
    <div className="mt-8">
      <div className="mb-3 flex items-end justify-between">
        <div>
          <span className="font-mono text-[11px] tracking-[1px] text-muted">CONNECTORS</span>
          <h2 style={{ margin: 0, fontSize: 18 }}>MCP servers</h2>
        </div>
        <button className="primary" onClick={() => setDraft(emptyDraft())}><Plus size={13} />Add server</button>
      </div>

      {servers.length === 0 ? (
        <div className="grid place-items-center rounded-[16px] border border-dashed border-line bg-panel p-6 text-center text-muted">
          <Server size={22} className="mb-2 opacity-50" />
          <p className="text-[12px]">No MCP servers yet. Add one, test it, then import its tools.</p>
        </div>
      ) : (
        <div className="grid max-w-[1100px] grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {servers.map((s) => (
            <div key={s.id} className="rounded-[16px] border border-line bg-panel p-[22px]">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <b className="text-[13px]">{s.name || s.id}</b>
                  <span className="mt-1 block truncate font-mono text-[11px] text-muted">{s.command} {(s.args ?? []).join(' ')}</span>
                </div>
                <div className="flex shrink-0 gap-1.5">
                  <button title="Test connection" className="secondary grid h-8 w-8 place-items-center p-0" onClick={() => test(s.id)} disabled={busy === s.id}>
                    <RefreshCw size={12} className={busy === s.id ? 'animate-spin' : ''} />
                  </button>
                  <button title="Sync tools" className="secondary grid h-8 w-8 place-items-center p-0" onClick={() => syncTools(s.id)} disabled={busy === s.id}>
                    <Upload size={12} />
                  </button>
                  <button title="Configure" className="secondary grid h-8 w-8 place-items-center p-0" onClick={() => setDraft({ id: s.id, name: s.name, command: s.command, args: (s.args ?? []).join(' '), env: envToLines(s.env), enabled: s.enabled })}>
                    <Server size={12} />
                  </button>
                  <button title="Delete" className="secondary grid h-8 w-8 place-items-center p-0" onClick={() => remove(s.id)}>
                    <Trash2 size={12} />
                  </button>
                </div>
              </div>

              {found[s.id] && (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {found[s.id].length === 0
                    ? <span className="text-[11px] text-[#f87171]">No tools returned.</span>
                    : found[s.id].map((t) => (
                      <span key={t.name} title={t.description} className="rounded bg-panel2 px-2 py-1 font-mono text-[11px] text-muted">{t.name}</span>
                    ))}
                </div>
              )}

              {errors[s.id] && (
                <pre className="mt-3 max-h-[200px] overflow-y-auto whitespace-pre-wrap break-words rounded-md border border-[#f87171]/30 bg-panel2 p-2.5 font-mono text-[11px] leading-[1.55] text-[#f87171]">{errors[s.id]}</pre>
              )}
            </div>
          ))}
        </div>
      )}

      {draft && (
        <Drawer
          title={draft.id ? `${draft.name || draft.id} — MCP server` : 'New MCP server'}
          onClose={() => setDraft(null)}
          initialWidth={Math.round(window.innerWidth / 2)}
          resizable
          headerAction={<button className="primary" disabled={saving} onClick={save}><Check size={13} />{saving ? 'Saving…' : 'Save'}</button>}
        >
          <div className="w-full rounded-[16px] border border-line bg-panel p-[22px]">
            <p className="text-[12px] leading-[1.6] text-muted" style={{ margin: '0 0 14px' }}>
              Runs the server's command locally and speaks MCP over stdio. Secrets belong in ENV. Saving imports its tools automatically.
            </p>

            <label className="mt-0 mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">NAME</label>
            <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} placeholder="Notion" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid" />

            <label className="mt-4 mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">COMMAND</label>
            <input value={draft.command} onChange={(e) => setDraft({ ...draft, command: e.target.value })} placeholder="npx" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 font-mono text-[12px] text-text outline-none focus:border-mid" />

            <label className="mt-4 mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">ARGS</label>
            <input value={draft.args} onChange={(e) => setDraft({ ...draft, args: e.target.value })} placeholder="-y @notionhq/notion-mcp-server" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 font-mono text-[12px] text-text outline-none focus:border-mid" />
            <p className="mt-1.5 text-[11px] text-muted">Space-separated. Quoted arguments aren't supported yet.</p>

            <label className="mt-4 mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">ENV</label>
            <textarea value={draft.env} onChange={(e) => setDraft({ ...draft, env: e.target.value })} rows={3} placeholder={'NOTION_TOKEN=ntn_…'} className="w-full resize-none rounded-md border border-line bg-panel2 px-3 py-2 font-mono text-[12px] text-text outline-none focus:border-mid" />
            <p className="mt-1.5 text-[11px] text-muted">One KEY=VALUE per line. Values are stored locally and never shown again.</p>
          </div>
        </Drawer>
      )}
    </div>
  );
}
