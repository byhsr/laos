import { useEffect, useState } from 'react';
import { Link2, PlugZap, Plus, RefreshCw, Server, Trash2, Upload } from 'lucide-react';
import { deleteMcpServer, importMcpTools, listMcpServers, onMcpChanged, remoteMcpConnect, remoteMcpDisconnect, remoteMcpSetToken, saveMcpServer, testMcpServer, type McpServer, type McpToolInfo } from '../../runtime';
import { toast } from '../../hooks/useToast';
import { useToolsStore } from '../../hooks/useTools';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS } from '../ui/Input';

type Transport = 'stdio' | 'http';
type Draft = { id: string; name: string; transport: Transport; command: string; args: string; env: string; url: string; token: string; connected: boolean; enabled: boolean };

const emptyDraft = (): Draft => ({ id: '', name: '', transport: 'stdio', command: '', args: '', env: '', url: '', token: '', connected: false, enabled: true });

// "KEY=VALUE" per line <-> the env object the backend stores.
const parseEnvLines = (text: string): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const t = line.trim();
    if (!t || t.startsWith('#')) continue;
    const i = t.indexOf('=');
    if (i <= 0) continue;
    // A stored secret renders as the backend's "••••••••" marker. Strip any of
    // those bullets left in the value, so typing around the marker yields the real
    // token and an untouched marker (now empty) keeps the stored secret.
    const value = t.slice(i + 1).trim().replace(/•/g, '').trim();
    out[t.slice(0, i).trim()] = value;
  }
  return out;
};
const envToLines = (env: Record<string, string>) =>
  Object.entries(env ?? {}).map(([k, v]) => `${k}=${v}`).join('\n');

// MCP connectors: point at a local server (stdio) or a hosted one (HTTP + OAuth),
// test it, then import its tools.
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

  // A remote sign-in completes in the backend after the browser round-trip; when
  // it lands, refresh the list. The outcome itself arrives as a notification (which
  // the shell already toasts), so this only keeps the connection state current.
  useEffect(() => onMcpChanged(() => { void load(); }), []);

  const save = async () => {
    if (!draft) return;
    const isRemote = draft.transport === 'http';
    if (isRemote && !draft.url.trim()) { toast('a remote server needs a URL', 'error'); return; }
    if (!isRemote && !draft.command.trim()) { toast('a local server needs a command to run', 'error'); return; }
    setSaving(true);
    try {
      const id = await saveMcpServer({
        id: draft.id,
        name: draft.name,
        transport: draft.transport,
        command: isRemote ? '' : draft.command.trim(),
        args: isRemote ? [] : draft.args.split(/\s+/).filter(Boolean),
        env: isRemote ? {} : parseEnvLines(draft.env),
        url: isRemote ? draft.url.trim() : '',
        connected: draft.connected,
        enabled: draft.enabled,
      });
      toast('mcp server saved', 'success');
      setDraft(null);
      await load();
      // A stdio server registers its tools straight away. A remote one can only
      // do that once it is authenticated — sign in first, then sync.
      if (isRemote) toast('now click connect to sign in, then sync tools', 'success');
      else await syncTools(id);
    } catch (e) {
      toast(typeof e === 'string' ? e : 'could not save the MCP server', 'error');
    } finally {
      setSaving(false);
    }
  };

  const connect = async (id: string) => {
    setBusy(id);
    try {
      await remoteMcpConnect(id);
      toast('sign-in opened in your browser — finish there, then sync tools', 'success');
    } catch (e) {
      const msg = typeof e === 'string' ? e : 'could not start the sign-in flow';
      setErrors((err) => ({ ...err, [id]: msg }));
      toast(msg, 'error');
    } finally {
      setBusy(null);
    }
  };

  const useToken = async (id: string, token: string) => {
    if (!token.trim()) { toast('paste a token first', 'error'); return; }
    setBusy(id);
    try {
      await remoteMcpSetToken(id, token.trim());
      await load();
      toast('token saved', 'success');
    } catch (e) {
      toast(typeof e === 'string' ? e : 'could not save the token', 'error');
    } finally { setBusy(null); }
  };

  const disconnect = async (id: string) => {
    await remoteMcpDisconnect(id);
    await load();
    toast('disconnected', 'success');
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
  const editing = draft?.id ? servers.find((s) => s.id === draft.id) : undefined;

  return (
    <div className="mt-8">
      <div className="mb-2.5 flex items-center justify-between gap-3">
        <span className={GROUP_LABEL_CLS}>mcp servers</span>
        <Button variant="primary" icon={<Plus size={13} />} onClick={() => setDraft(emptyDraft())}>add server</Button>
      </div>
      <p className="mt-0 mb-2.5 text-[11px] leading-[1.6] text-muted">
        add one server per account — e.g. <span className="font-mono">Notion — personal</span> and <span className="font-mono">Notion — work</span>. Duplicate names are fine: each server keeps its own tools and its own <span className="font-mono">env</span>.
      </p>

      {servers.length === 0 ? (
        <div className="grid place-items-center rounded-xl border border-dashed border-border p-6 text-center text-muted">
          <Server size={20} className="mb-2 opacity-50" />
          <p className="m-0 text-[12px]">No MCP servers yet. Add one, test it, then import its tools.</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-2.5 md:grid-cols-2 xl:grid-cols-3">
          {servers.map((s) => {
            const remote = s.transport === 'http';
            return (
              <Card key={s.id} className="flex flex-col gap-3 hover:bg-surface">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <b className="block truncate font-mono text-[11px] text-foreground">{s.name || s.id}</b>
                    <span className="mt-0.5 block truncate font-mono text-[10px] text-muted">
                      {remote ? s.url : `${s.command} ${(s.args ?? []).join(' ')}`}
                    </span>
                    {remote && (
                      <span className={`mt-1 inline-block font-mono text-[10px] ${s.connected ? 'text-foreground' : 'text-muted'}`}>
                        {s.connected ? '● connected' : '○ not connected'}
                      </span>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1.5">
                    {remote && (
                      <IconButton label={s.connected ? 'reconnect' : 'connect'} onClick={() => connect(s.id)} disabled={busy === s.id}>
                        <PlugZap size={12} />
                      </IconButton>
                    )}
                    <IconButton label="test connection" onClick={() => test(s.id)} disabled={busy === s.id}>
                      <RefreshCw size={12} className={busy === s.id ? 'animate-spin' : ''} />
                    </IconButton>
                    <IconButton label="sync tools" onClick={() => syncTools(s.id)} disabled={busy === s.id}><Upload size={12} /></IconButton>
                    <IconButton label="configure" onClick={() => setDraft({ id: s.id, name: s.name, transport: remote ? 'http' : 'stdio', command: s.command, args: (s.args ?? []).join(' '), env: envToLines(s.env), url: s.url, token: '', connected: s.connected, enabled: s.enabled })}><Server size={12} /></IconButton>
                    {remote && s.connected && <IconButton label="disconnect" onClick={() => disconnect(s.id)}><Link2 size={12} /></IconButton>}
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
            );
          })}
        </div>
      )}

      {draft && (
        <Modal
          title={draft.id ? `${draft.name || draft.id} — mcp server` : 'new mcp server'}
          onClose={() => setDraft(null)}
          headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
        >
          <div className="mb-3.5 flex gap-2">
            <Button variant={draft.transport === 'stdio' ? 'primary' : 'default'} onClick={() => setDraft({ ...draft, transport: 'stdio' })}>local (stdio)</Button>
            <Button variant={draft.transport === 'http' ? 'primary' : 'default'} onClick={() => setDraft({ ...draft, transport: 'http' })}>remote (http + oauth)</Button>
          </div>

          <label className={FIELD_LABEL_CLS}>name</label>
          <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} placeholder={draft.transport === 'http' ? 'Monid' : 'Notion'} className={INPUT_CLS} />

          {draft.transport === 'stdio' ? (
            <>
              <p className="mt-3 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
                Runs the server's command locally and speaks MCP over stdio. Secrets belong in env. Saving imports its tools automatically.
              </p>
              <label className={FIELD_LABEL_CLS}>command</label>
              <input value={draft.command} onChange={(e) => setDraft({ ...draft, command: e.target.value })} placeholder="npx" className={INPUT_CLS} />

              <label className={label}>args</label>
              <input value={draft.args} onChange={(e) => setDraft({ ...draft, args: e.target.value })} placeholder="-y @notionhq/notion-mcp-server" className={INPUT_CLS} />
              <p className="mt-1.5 mb-0 font-mono text-[10px] text-muted">Space-separated. Quoted arguments aren't supported yet.</p>

              <label className={label}>env</label>
              <textarea value={draft.env} onChange={(e) => setDraft({ ...draft, env: e.target.value })} rows={3} placeholder={'NOTION_TOKEN=ntn_…'} className={`${INPUT_CLS} resize-none`} />
              <p className="mt-1.5 mb-0 font-mono text-[10px] text-muted">One KEY=VALUE per line. A saved secret shows as •••••••• — type over it to replace, or leave it to keep the stored value.</p>
            </>
          ) : (
            <>
              <p className="mt-3 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
                Connects to a hosted MCP server over Streamable HTTP and signs in with OAuth. Save first, then connect — e.g. <span className="text-foreground">https://mcp.monid.ai/v1</span> or <span className="text-foreground">https://treg.to/mcp/</span>.
              </p>
              <label className={FIELD_LABEL_CLS}>server url</label>
              <input value={draft.url} onChange={(e) => setDraft({ ...draft, url: e.target.value })} placeholder="https://mcp.monid.ai/v1" className={INPUT_CLS} />

              <div className="mt-4 flex items-center gap-2">
                <Button variant="primary" icon={<PlugZap size={13} />} disabled={!draft.id || busy === draft.id} onClick={() => draft.id && connect(draft.id)}>
                  {editing?.connected ? 'reconnect' : 'connect'}
                </Button>
                <span className="font-mono text-[10px] text-muted">{draft.id ? (editing?.connected ? '● connected' : '○ not connected') : 'save first to connect'}</span>
              </div>

              <label className={label}>or paste a token (optional)</label>
              <div className="flex gap-2">
                <input value={draft.token} onChange={(e) => setDraft({ ...draft, token: e.target.value })} placeholder="bearer token" className={INPUT_CLS} />
                <Button variant="default" disabled={!draft.id || busy === draft.id} onClick={() => draft.id && useToken(draft.id, draft.token)}>use</Button>
              </div>
              <p className="mt-1.5 mb-0 font-mono text-[10px] text-muted">For a host whose OAuth discovery isn't available. Same as the connect flow, without the browser.</p>
            </>
          )}
        </Modal>
      )}
    </div>
  );
}
