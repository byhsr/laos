import { useState } from 'react';
import { Check, Play, Plus, Trash2, Wrench } from 'lucide-react';
import type { Integration, Tool, ToolAction, ToolAuthType, ToolParam } from '../../types';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Select } from '../ui/Select';
import { Checkbox } from '../ui/Checkbox';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, INPUT_CLS } from '../ui/Input';
import { VAULT_MASK, isSecretName } from '../../vault';
import { testTool } from '../../runtime';

export const TOOL_KINDS: { kind: string; integration: string; desc: string }[] = [
  { kind: 'api', integration: 'http', desc: 'Call any REST API — one endpoint, or a suite of actions' },
  { kind: 'http_get', integration: 'http', desc: 'Permission-scoped GET request' },
  { kind: 'read_file', integration: 'builtin', desc: 'Read from agent files dir' },
  { kind: 'write_file', integration: 'builtin', desc: 'Write to agent files dir' },
];

export function ToolsView({ tools, integrations, onAdd, onEdit, onDelete }: {
  tools: Tool[]; integrations: Integration[]; onAdd: () => void; onEdit: (t: Tool) => void; onDelete: (id: string) => Promise<void>;
}) {
  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button variant="primary" icon={<Check size={13} />} onClick={onAdd}>add tool</Button>
      </div>

      <div className="grid gap-2">
        {tools.map((t) => {
          const integration = integrations.find((i) => i.id === t.integrationId);
          // MCP tools are managed by their server, so name the server and drop
          // the generic Edit form (its kind has no MCP branch).
          const source = t.kind === 'mcp'
            ? `mcp · ${String(t.config?.serverName ?? t.integrationId)}`
            : `${(t.kind ?? '').replace('_', ' ')} · ${integration?.name ?? t.integrationId}`;
          const actions = t.config?.actions ?? [];
          return (
            <Card key={t.id} className="flex items-center gap-4 hover:bg-surface">
              <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-muted"><Wrench size={16} /></span>
              <div className="min-w-0 flex-1">
                <b className="block truncate font-mono text-[11px] text-foreground">{t.name}</b>
                <span className="block truncate font-mono text-[10px] text-muted">{source}{t.description ? ` · ${t.description}` : ''}</span>
                {t.kind === 'api' && (
                  <span className="mt-0.5 block truncate font-mono text-[10px] text-muted">
                    {actions.length > 0
                      ? `${actions.length} action${actions.length === 1 ? '' : 's'} · ${t.config?.baseUrl ?? ''}`
                      : `${t.config?.method ?? 'GET'} ${t.config?.url ?? ''}`}
                  </span>
                )}
              </div>
              <span className="flex shrink-0 items-center gap-1.5 font-mono text-[10px] text-muted">
                <i className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${t.enabled ? 'bg-foreground' : 'bg-muted'}`} />
                {t.enabled ? 'enabled' : 'disabled'}
              </span>
              <div className="flex shrink-0 items-center gap-1.5">
                {t.kind !== 'mcp' && <Button onClick={() => onEdit(t)}>edit</Button>}
                <IconButton label="delete tool" onClick={() => onDelete(t.id)}><Trash2 size={12} /></IconButton>
              </div>
            </Card>
          );
        })}
        {tools.length === 0 && <p className="m-0 font-mono text-[11px] text-muted">no tools yet — add one to start attaching capabilities to agents</p>}
      </div>
    </>
  );
}

const PARAM_TYPES = ['string', 'integer', 'number', 'boolean'];
const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
// Where a parameter lands in the request. "url (raw)" is the original behaviour.
const PARAM_IN = [
  { value: '', label: 'url (raw)' },
  { value: 'path', label: 'path segment' },
  { value: 'query', label: 'query' },
  { value: 'body', label: 'json body' },
  { value: 'header', label: 'header' },
];
const AUTH_TYPES: { value: ToolAuthType; label: string }[] = [
  { value: 'none', label: 'none' },
  { value: 'bearer', label: 'bearer token' },
  { value: 'header', label: 'api key in a header' },
  { value: 'query', label: 'api key in the query' },
];
const APPROVALS = [
  { value: 'writes', label: 'only writes (POST / PUT / PATCH / DELETE)' },
  { value: 'always', label: 'every call' },
  { value: 'never', label: 'never — always run it' },
];

const label = `${FIELD_LABEL_CLS} mt-4`;
const addBtn = 'justify-self-start';

// A name the model can be given: providers accept only [a-zA-Z0-9_-].
const fnNameFor = (name: string, fallback: string) =>
  name.trim().toLowerCase().replace(/[^a-z0-9_]+/g, '_').replace(/^_|_$/g, '') || fallback;

function HeaderEditor({ headers, onChange }: { headers: { name: string; value: string }[]; onChange: (h: { name: string; value: string }[]) => void }) {
  return (
    <div className="grid gap-1.5">
      {headers.map((h, i) => {
        // A header whose name looks like a credential is write-only: the backend
        // stores it in the OS credential store and only ever hands back the "set"
        // marker, so it renders as a password field.
        const secret = isSecretName(h.name);
        return (
          <div key={i} className="flex flex-wrap items-center gap-1.5">
            <input value={h.name} onChange={(e) => { const hs = [...headers]; hs[i] = { ...hs[i], name: e.target.value }; onChange(hs); }} placeholder="Header" className={`${INPUT_CLS} min-w-[120px] flex-1`} />
            <input type={secret ? 'password' : 'text'} value={h.value} onChange={(e) => { const hs = [...headers]; hs[i] = { ...hs[i], value: e.target.value }; onChange(hs); }} placeholder={h.value === VAULT_MASK ? 'saved — type to replace' : 'Value (e.g. Bearer {apiKey})'} className={`${INPUT_CLS} min-w-[160px] flex-[2]`} />
            <IconButton label="remove header" onClick={() => onChange(headers.filter((_, j) => j !== i))}><Trash2 size={12} /></IconButton>
          </div>
        );
      })}
      <Button className={addBtn} icon={<Plus size={12} />} onClick={() => onChange([...headers, { name: '', value: '' }])}>add header</Button>
    </div>
  );
}

function ParamEditor({ params, onChange }: { params: ToolParam[]; onChange: (p: ToolParam[]) => void }) {
  return (
    <div className="grid gap-2">
      {params.map((p, i) => (
        <div key={i} className="rounded-lg border border-border bg-background p-2.5">
          <div className="flex flex-wrap items-center gap-1.5">
            <input value={p.name} onChange={(e) => { const ps = [...params]; ps[i] = { ...ps[i], name: e.target.value }; onChange(ps); }} placeholder="paramName" className={`${INPUT_CLS} min-w-[120px] flex-1`} />
            <div className="w-28 shrink-0">
              <Select value={p.type} options={PARAM_TYPES.map((t) => ({ value: t, label: t }))} onChange={(v) => { const ps = [...params]; ps[i] = { ...ps[i], type: v }; onChange(ps); }} />
            </div>
            <div className="w-36 shrink-0">
              <Select value={p.in ?? ''} options={PARAM_IN} onChange={(v) => { const ps = [...params]; ps[i] = { ...ps[i], in: (v || undefined) as ToolParam['in'] }; onChange(ps); }} />
            </div>
            <IconButton label="remove parameter" onClick={() => onChange(params.filter((_, j) => j !== i))}><Trash2 size={12} /></IconButton>
          </div>
          <input value={p.description} onChange={(e) => { const ps = [...params]; ps[i] = { ...ps[i], description: e.target.value }; onChange(ps); }} placeholder="What is this param? The LLM uses this to fill it." className={`${INPUT_CLS} mt-1.5`} />
          <div className="-mx-2 mt-1">
            <Checkbox
              checked={p.required}
              onChange={(next) => { const ps = [...params]; ps[i] = { ...ps[i], required: next }; onChange(ps); }}
              label="required"
            />
          </div>
        </div>
      ))}
      <Button className={addBtn} icon={<Plus size={12} />} onClick={() => onChange([...params, { name: '', type: 'string', description: '', required: false }])}>add parameter</Button>
    </div>
  );
}

function TestResult({ result }: { result: { action: string; text: string; error?: boolean } | null }) {
  if (!result) return null;
  return (
    <div className="mt-3 rounded-lg border border-border bg-background p-2.5">
      <b className="block font-mono text-[10px] lowercase text-muted">{result.action ? `${result.action} — ` : ''}{result.error ? 'failed' : 'response'}</b>
      <pre className={`m-0 mt-1 max-h-56 overflow-auto whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed ${result.error ? 'text-danger' : 'text-foreground/80'}`}>{result.text}</pre>
    </div>
  );
}

export function ToolFormDrawer({ editing, isNew, integrations, onClose, onSave }: {
  editing: Tool; isNew: boolean; integrations: Integration[]; onClose: () => void; onSave: (t: Tool) => Promise<void>;
}) {
  const [form, setForm] = useState<Tool>(editing);
  const [mode, setMode] = useState<'single' | 'suite'>((editing.config.actions?.length ?? 0) > 0 ? 'suite' : 'single');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const [testing, setTesting] = useState<string | null>(null);
  const [testOut, setTestOut] = useState<{ action: string; text: string; error?: boolean } | null>(null);
  const kindFor = (kind: string) => TOOL_KINDS.find((k) => k.kind === kind);
  const isApi = form.kind === 'api';
  const isSuite = isApi && mode === 'suite';
  const actions = form.config.actions ?? [];

  const save = async () => {
    if (!form.name.trim() || !form.kind.trim()) { setError('Tool name and kind are required.'); return; }
    if (isSuite && !form.config.baseUrl?.trim()) { setError('A suite needs a base URL.'); return; }
    if (!isSuite && isApi && !form.config.url?.trim()) { setError('API tools need a URL.'); return; }
    const id = form.id || `${form.integrationId}-${form.kind}-${Date.now()}`;
    setSaving(true); setError(undefined);
    try {
      // The LLM sees the tool under a stable function name derived from the display
      // name; on a suite it is also the prefix every action is named with.
      const fnName = fnNameFor(form.name, `api_${id}`);
      const next = { ...form, id, description: form.description.trim(), config: { ...form.config, name: fnName } };
      if (!isSuite) next.config.actions = undefined;
      await onSave(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save tool.');
    } finally {
      setSaving(false);
    }
  };

  // Switching mode keeps the config but adds or drops the suite fields, so the
  // runtime reads the tool as whichever shape the user picked.
  const switchMode = (next: 'single' | 'suite') => {
    if (next === mode) return;
    setTestOut(null);
    if (next === 'suite') {
      setCfg({
        baseUrl: form.config.baseUrl ?? '',
        auth: form.config.auth ?? { type: 'none' },
        approval: form.config.approval ?? 'writes',
        actions: actions.length > 0 ? actions : [{ name: '', method: 'GET', path: '', description: '', params: [] }],
      });
    } else {
      const first = actions[0];
      setCfg({
        actions: undefined,
        url: form.config.url?.trim() ? form.config.url : `${form.config.baseUrl ?? ''}${first?.path ?? ''}`,
        method: form.config.method ?? first?.method ?? 'GET',
      });
    }
    setMode(next);
  };

  const setCfg = (patch: Partial<Tool['config']>) => setForm({ ...form, config: { ...form.config, ...patch } });
  const setParams = (params: ToolParam[]) => setCfg({ params });
  const setHeaders = (headers: { name: string; value: string }[]) => setCfg({ headers });
  const setActions = (next: ToolAction[]) => setCfg({ actions: next });
  const patchAction = (i: number, patch: Partial<ToolAction>) => setActions(actions.map((a, j) => (j === i ? { ...a, ...patch } : a)));
  const setAuth = (patch: Partial<NonNullable<Tool['config']['auth']>>) => setCfg({ auth: { type: 'none', ...form.config.auth, ...patch } });
  const auth = form.config.auth ?? { type: 'none' as ToolAuthType };

  // Tests through the backend's own builder, so the response is exactly what an
  // agent would get — including auth resolution and query encoding.
  const runTest = async (action?: string) => {
    const key = action ?? '';
    setTesting(key); setTestOut(null);
    try {
      const id = form.id || `draft-${Date.now()}`;
      const text = await testTool({ ...form, id, config: { ...form.config, name: fnNameFor(form.name, `api_${id}`) } }, action);
      setTestOut({ action: key, text });
    } catch (e) {
      setTestOut({ action: key, text: e instanceof Error ? e.message : String(e), error: true });
    } finally {
      setTesting(null);
    }
  };

  return (
    <Modal
      title={isNew ? 'add tool' : `edit ${form.name}`}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
    >
      <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
        {isSuite
          ? 'One tool, many actions — the shape an integration has. Every action shares the base URL, headers and auth, and the agent sees each one as {name}_{action}.'
          : isApi
            ? 'Configure a REST endpoint. The LLM will see the params below as callable arguments, and you can reference them in the URL, headers, and body with {paramName}.'
            : 'Tools are reusable capabilities you attach to any agent.'}
      </p>

      <label className={FIELD_LABEL_CLS}>name</label>
      <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder={isApi ? 'e.g. Notion' : 'e.g. Web Search'} className={INPUT_CLS} />

      <label className={label}>kind</label>
      <Select
        value={form.kind}
        options={TOOL_KINDS.map((k) => ({ value: k.kind, label: `${k.kind.replace('_', ' ')} — ${k.desc}` }))}
        onChange={(v) => { const k = kindFor(v); setForm({ ...form, kind: v, integrationId: k ? k.integration : form.integrationId }); }}
      />

      {isApi && (
        <>
          <label className={label}>shape</label>
          <Select
            value={mode}
            options={[
              { value: 'single', label: 'single endpoint' },
              { value: 'suite', label: 'api suite — a base URL and several actions' },
            ]}
            onChange={(v) => switchMode(v as 'single' | 'suite')}
          />
        </>
      )}

      {!isApi && (
        <>
          <label className={label}>backed by</label>
          <Select
            value={form.integrationId}
            options={integrations.map((i) => ({ value: i.id, label: i.name }))}
            onChange={(v) => setForm({ ...form, integrationId: v })}
          />
        </>
      )}

      {isApi && isSuite && (
        <>
          <label className={label}>base url</label>
          <input
            value={form.config.baseUrl ?? ''}
            onChange={(e) => setCfg({ baseUrl: e.target.value })}
            placeholder="https://api.notion.com/v1"
            className={INPUT_CLS}
          />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">Each action's path is appended to this.</p>

          <label className={label}>auth</label>
          <Select value={auth.type} options={AUTH_TYPES} onChange={(v) => setAuth({ type: v as ToolAuthType })} />
          {auth.type !== 'none' && (
            <div className="mt-1.5 grid gap-1.5">
              {auth.type !== 'bearer' && (
                <input
                  value={auth.name ?? ''}
                  onChange={(e) => setAuth({ name: e.target.value })}
                  placeholder={auth.type === 'query' ? 'query key (api_key)' : 'header name (x-api-key)'}
                  className={INPUT_CLS}
                />
              )}
              <input
                type="password"
                value={auth.value ?? ''}
                onChange={(e) => setAuth({ value: e.target.value })}
                placeholder={auth.value === VAULT_MASK ? 'saved — type to replace' : auth.type === 'bearer' ? 'token' : 'key'}
                className={INPUT_CLS}
              />
              <p className="m-0 font-mono text-[10px] leading-relaxed text-muted">
                Stored in the OS credential store, never in the tool's config.
              </p>
            </div>
          )}

          <label className={label}>ask before running</label>
          <Select
            value={form.config.approval ?? 'writes'}
            options={APPROVALS}
            onChange={(v) => setCfg({ approval: v as Tool['config']['approval'] })}
          />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">Applies while autonomy is off.</p>

          <label className={label}>headers</label>
          <HeaderEditor headers={form.config.headers ?? []} onChange={setHeaders} />

          <label className={label}>actions</label>
          <div className="grid gap-2.5">
            {actions.map((a, i) => (
              <div key={i} className="rounded-lg border border-border bg-background p-2.5">
                <div className="flex flex-wrap items-center gap-1.5">
                  <input value={a.name} onChange={(e) => patchAction(i, { name: e.target.value })} placeholder="actionName" className={`${INPUT_CLS} min-w-[120px] flex-1`} />
                  <div className="w-28 shrink-0">
                    <Select value={a.method ?? 'GET'} options={METHODS.map((m) => ({ value: m, label: m }))} onChange={(v) => patchAction(i, { method: v })} />
                  </div>
                  <IconButton label="remove action" onClick={() => setActions(actions.filter((_, j) => j !== i))}><Trash2 size={12} /></IconButton>
                </div>
                <input value={a.path ?? ''} onChange={(e) => patchAction(i, { path: e.target.value })} placeholder="/pages/{pageId}" className={`${INPUT_CLS} mt-1.5`} />
                <input value={a.description ?? ''} onChange={(e) => patchAction(i, { description: e.target.value })} placeholder="What does this action do? The LLM uses this." className={`${INPUT_CLS} mt-1.5`} />
                <textarea value={a.body ?? ''} onChange={(e) => patchAction(i, { body: e.target.value })} rows={2} placeholder='{"query": "{q}"}' className={`${INPUT_CLS} mt-1.5 resize-y`} />
                <div className="mt-2">
                  <ParamEditor params={a.params ?? []} onChange={(ps) => patchAction(i, { params: ps })} />
                </div>
                <div className="mt-2">
                  <Button icon={<Play size={12} />} disabled={!a.name || testing !== null} onClick={() => runTest(a.name)}>
                    {testing === a.name ? 'running…' : `test ${form.config.name ? `${form.config.name}_` : ''}${a.name}`}
                  </Button>
                </div>
              </div>
            ))}
            <Button className={addBtn} icon={<Plus size={12} />} onClick={() => setActions([...actions, { name: '', method: 'GET', path: '', description: '', params: [] }])}>add action</Button>
          </div>

          <div className="mt-3 flex justify-end">
            <Button icon={<Play size={12} />} disabled={actions.length === 0 || testing !== null} onClick={() => runTest(actions[0]?.name)}>
              {testing !== null ? 'running…' : 'test first action'}
            </Button>
          </div>
          <TestResult result={testOut} />
        </>
      )}

      {isApi && !isSuite && (
        <>
          <label className={label}>method</label>
          <Select
            value={form.config.method ?? 'GET'}
            options={METHODS.map((m) => ({ value: m, label: m }))}
            onChange={(v) => setCfg({ method: v })}
          />

          <label className={label}>url</label>
          <input
            value={form.config.url ?? ''}
            onChange={(e) => setCfg({ url: e.target.value })}
            placeholder="https://api.example.com/users/{id}"
            className={INPUT_CLS}
          />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">Use {'{param}'} placeholders — they get filled from the LLM's arguments.</p>

          <label className={label}>ask before running</label>
          <Select
            value={form.config.approval ?? 'writes'}
            options={APPROVALS}
            onChange={(v) => setCfg({ approval: v as Tool['config']['approval'] })}
          />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">Applies while autonomy is off.</p>

          <label className={label}>headers</label>
          <HeaderEditor headers={form.config.headers ?? []} onChange={setHeaders} />

          <label className={label}>body (json, optional)</label>
          <textarea
            value={form.config.body ?? ''}
            onChange={(e) => setCfg({ body: e.target.value })}
            rows={3}
            placeholder='{"name": "{name}", "role": "admin"}'
            className={`${INPUT_CLS} resize-y`}
          />

          <label className={label}>parameters (what the agent can pass)</label>
          <ParamEditor params={form.config.params ?? []} onChange={setParams} />

          <div className="mt-3 flex justify-end">
            <Button icon={<Play size={12} />} disabled={testing !== null} onClick={() => runTest()}>
              {testing !== null ? 'running…' : 'test request'}
            </Button>
          </div>
          <TestResult result={testOut} />
        </>
      )}

      <label className={label}>description</label>
      <textarea value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} rows={2} placeholder="When should the agent use this? What does it return?" className={`${INPUT_CLS} resize-y`} />

      <div className="-mx-2 mt-3.5">
        <Checkbox checked={form.enabled} onChange={(next) => setForm({ ...form, enabled: next })} label="enabled" hint="available to agents" />
      </div>

      {error && <p className="mt-2.5 mb-0 font-mono text-[10px] text-danger">{error}</p>}
    </Modal>
  );
}
