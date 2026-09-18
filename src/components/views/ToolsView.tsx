import { useState } from 'react';
import { Check, Plus, Trash2, Wrench } from 'lucide-react';
import type { Integration, Tool, ToolParam } from '../../types';
import { Drawer } from '../ui/Drawer';
import { Dropdown } from '../ui/Dropdown';

export const TOOL_KINDS: { kind: string; integration: string; desc: string }[] = [
  { kind: 'api', integration: 'http', desc: 'Call any REST API with configured params' },
  { kind: 'http_get', integration: 'http', desc: 'Permission-scoped GET request' },
  { kind: 'read_file', integration: 'builtin', desc: 'Read from agent files dir' },
  { kind: 'write_file', integration: 'builtin', desc: 'Write to agent files dir' },
];

export function ToolsView({ tools, integrations, onAdd, onEdit, onDelete, embedded = false }: {
  tools: Tool[]; integrations: Integration[]; onAdd: () => void; onEdit: (t: Tool) => void; onDelete: (id: string) => Promise<void>; embedded?: boolean;
}) {
  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        {!embedded && <div><span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span><h1 style={{ margin: 0, fontSize: 24 }}>Tools</h1></div>}
        <button className="primary ml-auto" onClick={onAdd}><Check size={13} />Add tool</button>
      </header>
      <div className="grid max-w-[900px] gap-2.5">
        {tools.map((t) => {
          const integration = integrations.find((i) => i.id === t.integrationId);
          return (
            <div key={t.id} className="flex items-center gap-[15px] rounded-[9px] border border-line bg-panel p-[18px]">
              <span className="grid h-10 w-10 place-items-center rounded-lg bg-panel2 text-[20px] text-muted"><Wrench size={18} /></span>
              <div className="flex-1">
                <b style={{ fontSize: 13 }}>{t.name}</b>
                <span className="block text-[11px] text-muted">{t.kind.replace('_', ' ')} · {integration?.name ?? t.integrationId}{t.description ? ` · ${t.description}` : ''}</span>
                {t.kind === 'api' && (
                  <span className="mt-1 block font-mono text-[10px] text-muted">{t.config.method ?? 'GET'} {t.config.url ?? ''}</span>
                )}
              </div>
              <span className="mr-[7px] text-[10px] text-muted"><i className={`mr-1.5 inline-block h-[7px] w-[7px] rounded-full ${t.enabled ? 'bg-[var(--green)]' : 'bg-[#f79009]'}`} />{t.enabled ? 'on' : 'off'}</span>
              <div style={{ display: 'flex', gap: 6 }}>
                <button className="secondary" onClick={() => onEdit(t)}>Edit</button>
                <button className="secondary" onClick={() => onDelete(t.id)}>Delete</button>
              </div>
            </div>
          );
        })}
        {tools.length === 0 && <p className="mt-[22px] text-[12px] text-muted">No tools yet. Add one to start attaching capabilities to agents.</p>}
      </div>
    </>
  );
}

const PARAM_TYPES = ['string', 'integer', 'number', 'boolean'];

export function ToolFormDrawer({ editing, isNew, integrations, onClose, onSave }: {
  editing: Tool; isNew: boolean; integrations: Integration[]; onClose: () => void; onSave: (t: Tool) => Promise<void>;
}) {
  const [form, setForm] = useState<Tool>(editing);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const kindFor = (kind: string) => TOOL_KINDS.find((k) => k.kind === kind);
  const isApi = form.kind === 'api';

  const save = async () => {
    if (!form.name.trim() || !form.kind.trim()) { setError('Tool name and kind are required.'); return; }
    if (isApi && !form.config.url?.trim()) { setError('API tools need a URL.'); return; }
    const id = form.id || `${form.integrationId}-${form.kind}-${Date.now()}`;
    setSaving(true); setError(undefined);
    try {
      // The LLM sees the tool under a stable function name derived from the display name.
      const fnName = form.name.trim().toLowerCase().replace(/[^a-z0-9_]+/g, '_').replace(/^_|_$/g, '') || `api_${id}`;
      await onSave({ ...form, id, description: form.description.trim(), config: { ...form.config, name: fnName } });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save tool.');
    } finally {
      setSaving(false);
    }
  };

  const setCfg = (patch: Partial<Tool['config']>) => setForm({ ...form, config: { ...form.config, ...patch } });
  const setParams = (params: ToolParam[]) => setCfg({ params });
  const setHeaders = (headers: { name: string; value: string }[]) => setCfg({ headers });

  const inputCls = 'w-full rounded-[6px] border border-line bg-panel2 px-3 py-2.5 text-text';
  const labelCls = 'mt-4 mb-1.5 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase';

  return (
    <Drawer
      title={isNew ? 'Add tool' : `Edit ${form.name}`}
      onClose={onClose}
      initialWidth={Math.round(window.innerWidth / 2)}
      resizable
      headerAction={<button className="primary" disabled={saving} onClick={save}><Check size={13} />{saving ? 'Saving…' : 'Save'}</button>}
    >
      <div className="w-full rounded-[10px] border border-line bg-panel p-[22px]">
        <p className="text-[12px] leading-[1.6] text-muted" style={{ margin: '0 0 14px' }}>
          {isApi
            ? 'Configure a REST endpoint. The LLM will see the params below as callable arguments, and you can reference them in the URL, headers, and body with {paramName}.'
            : 'Tools are reusable capabilities you attach to any agent.'}
        </p>

        <label className={labelCls}>NAME</label>
        <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder={isApi ? 'e.g. GitHub API' : 'e.g. Web Search'} className={inputCls} />

        <label className={labelCls}>KIND</label>
        <Dropdown
          value={form.kind}
          options={TOOL_KINDS.map((k) => ({ value: k.kind, label: `${k.kind.replace('_', ' ')} — ${k.desc}` }))}
          onChange={(v) => { const k = kindFor(v); setForm({ ...form, kind: v, integrationId: k ? k.integration : form.integrationId }); }}
        />

        {!isApi && (
          <>
            <label className={labelCls}>BACKED BY</label>
            <Dropdown
              value={form.integrationId}
              options={integrations.map((i) => ({ value: i.id, label: i.name }))}
              onChange={(v) => setForm({ ...form, integrationId: v })}
            />
          </>
        )}

        {isApi && (
          <>
            <label className={labelCls}>METHOD</label>
            <Dropdown
              value={form.config.method ?? 'GET'}
              options={['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map((m) => ({ value: m, label: m }))}
              onChange={(v) => setCfg({ method: v })}
            />

            <label className={labelCls}>URL</label>
            <input
              value={form.config.url ?? ''}
              onChange={(e) => setCfg({ url: e.target.value })}
              placeholder="https://api.example.com/users/{id}"
              className={inputCls}
            />
            <p className="text-[11px] leading-[1.5] text-muted" style={{ margin: '6px 0 0' }}>Use {'{param}'} placeholders — they get filled from the LLM's arguments.</p>

            <label className={labelCls}>HEADERS</label>
            <div className="grid gap-1.5">
              {(form.config.headers ?? []).map((h, i) => (
                <div key={i} className="flex flex-wrap gap-1.5">
                  <input value={h.name} onChange={(e) => { const hs = [...(form.config.headers ?? [])]; hs[i] = { ...hs[i], name: e.target.value }; setHeaders(hs); }} placeholder="Header" className={`${inputCls} min-w-[120px] flex-1`} />
                  <input value={h.value} onChange={(e) => { const hs = [...(form.config.headers ?? [])]; hs[i] = { ...hs[i], value: e.target.value }; setHeaders(hs); }} placeholder="Value (e.g. Bearer {apiKey})" className={`${inputCls} min-w-[160px] flex-[2]`} />
                  <button className="secondary shrink-0" onClick={() => setHeaders((form.config.headers ?? []).filter((_, j) => j !== i))}><Trash2 size={12} /></button>
                </div>
              ))}
              <button className="secondary justify-self-start" onClick={() => setHeaders([...(form.config.headers ?? []), { name: '', value: '' }])}><Plus size={12} />Add header</button>
            </div>

            <label className={labelCls}>BODY (JSON, optional)</label>
            <textarea
              value={form.config.body ?? ''}
              onChange={(e) => setCfg({ body: e.target.value })}
              rows={3}
              placeholder='{"name": "{name}", "role": "admin"}'
              className={`${inputCls} resize-y`}
            />

            <label className={labelCls}>PARAMETERS (what the agent can pass)</label>
            <div className="grid gap-2">
              {(form.config.params ?? []).map((p, i) => (
                <div key={i} className="rounded-[6px] border border-line bg-panel2 p-2.5">
                  <div className="flex flex-wrap gap-1.5">
                    <input value={p.name} onChange={(e) => { const ps = [...(form.config.params ?? [])]; ps[i] = { ...ps[i], name: e.target.value }; setParams(ps); }} placeholder="paramName" className={`${inputCls} min-w-[120px] flex-1`} />
                    <select value={p.type} onChange={(e) => { const ps = [...(form.config.params ?? [])]; ps[i] = { ...ps[i], type: e.target.value }; setParams(ps); }} className={`${inputCls} w-28 shrink-0`}>
                      {PARAM_TYPES.map((t) => <option key={t} value={t}>{t}</option>)}
                    </select>
                    <button className="secondary shrink-0" onClick={() => setParams((form.config.params ?? []).filter((_, j) => j !== i))}><Trash2 size={12} /></button>
                  </div>
                  <input value={p.description} onChange={(e) => { const ps = [...(form.config.params ?? [])]; ps[i] = { ...ps[i], description: e.target.value }; setParams(ps); }} placeholder="What is this param? The LLM uses this to fill it." className={`${inputCls} mt-1.5`} />
                  <label className="mt-1.5 flex items-center gap-1.5 text-[11px] text-muted">
                    <input type="checkbox" checked={p.required} onChange={(e) => { const ps = [...(form.config.params ?? [])]; ps[i] = { ...ps[i], required: e.target.checked }; setParams(ps); }} />Required
                  </label>
                </div>
              ))}
              <button className="secondary justify-self-start" onClick={() => setParams([...(form.config.params ?? []), { name: '', type: 'string', description: '', required: false }])}><Plus size={12} />Add parameter</button>
            </div>
          </>
        )}

        <label className={labelCls}>DESCRIPTION</label>
        <textarea value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} rows={2} placeholder="When should the agent use this? What does it return?" className={`${inputCls} resize-y`} />

        <div style={{ marginTop: 14, display: 'flex', alignItems: 'center', gap: 10 }}>
          <label style={{ margin: 0, display: 'flex', alignItems: 'center', gap: 6, fontSize: 12 }}>
            <input type="checkbox" checked={form.enabled} onChange={(e) => setForm({ ...form, enabled: e.target.checked })} />Enabled
          </label>
        </div>

        {error && <p style={{ fontSize: 11, color: '#f87171', margin: '10px 0 0' }}>{error}</p>}
      </div>
    </Drawer>
  );
}
