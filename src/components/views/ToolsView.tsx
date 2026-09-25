import { useState } from 'react';
import { Check, Plus, Trash2, Wrench } from 'lucide-react';
import type { Integration, Tool, ToolParam } from '../../types';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Select } from '../ui/Select';
import { Checkbox } from '../ui/Checkbox';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, INPUT_CLS } from '../ui/Input';

export const TOOL_KINDS: { kind: string; integration: string; desc: string }[] = [
  { kind: 'api', integration: 'http', desc: 'Call any REST API with configured params' },
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
            ? `mcp · ${String(t.config.serverName ?? t.integrationId)}`
            : `${t.kind.replace('_', ' ')} · ${integration?.name ?? t.integrationId}`;
          return (
            <Card key={t.id} className="flex items-center gap-4 hover:bg-surface">
              <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-muted"><Wrench size={16} /></span>
              <div className="min-w-0 flex-1">
                <b className="block truncate font-mono text-[11px] text-foreground">{t.name}</b>
                <span className="block truncate font-mono text-[10px] text-muted">{source}{t.description ? ` · ${t.description}` : ''}</span>
                {t.kind === 'api' && (
                  <span className="mt-0.5 block truncate font-mono text-[10px] text-muted">{t.config.method ?? 'GET'} {t.config.url ?? ''}</span>
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

  const label = `${FIELD_LABEL_CLS} mt-4`;
  const addBtn = 'justify-self-start';

  return (
    <Modal
      title={isNew ? 'add tool' : `edit ${form.name}`}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
    >
      <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
        {isApi
          ? 'Configure a REST endpoint. The LLM will see the params below as callable arguments, and you can reference them in the URL, headers, and body with {paramName}.'
          : 'Tools are reusable capabilities you attach to any agent.'}
      </p>

      <label className={FIELD_LABEL_CLS}>name</label>
      <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder={isApi ? 'e.g. GitHub API' : 'e.g. Web Search'} className={INPUT_CLS} />

      <label className={label}>kind</label>
      <Select
        value={form.kind}
        options={TOOL_KINDS.map((k) => ({ value: k.kind, label: `${k.kind.replace('_', ' ')} — ${k.desc}` }))}
        onChange={(v) => { const k = kindFor(v); setForm({ ...form, kind: v, integrationId: k ? k.integration : form.integrationId }); }}
      />

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

      {isApi && (
        <>
          <label className={label}>method</label>
          <Select
            value={form.config.method ?? 'GET'}
            options={['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map((m) => ({ value: m, label: m }))}
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

          <label className={label}>headers</label>
          <div className="grid gap-1.5">
            {(form.config.headers ?? []).map((h, i) => (
              <div key={i} className="flex flex-wrap items-center gap-1.5">
                <input value={h.name} onChange={(e) => { const hs = [...(form.config.headers ?? [])]; hs[i] = { ...hs[i], name: e.target.value }; setHeaders(hs); }} placeholder="Header" className={`${INPUT_CLS} min-w-[120px] flex-1`} />
                <input value={h.value} onChange={(e) => { const hs = [...(form.config.headers ?? [])]; hs[i] = { ...hs[i], value: e.target.value }; setHeaders(hs); }} placeholder="Value (e.g. Bearer {apiKey})" className={`${INPUT_CLS} min-w-[160px] flex-[2]`} />
                <IconButton label="remove header" onClick={() => setHeaders((form.config.headers ?? []).filter((_, j) => j !== i))}><Trash2 size={12} /></IconButton>
              </div>
            ))}
            <Button className={addBtn} icon={<Plus size={12} />} onClick={() => setHeaders([...(form.config.headers ?? []), { name: '', value: '' }])}>add header</Button>
          </div>

          <label className={label}>body (json, optional)</label>
          <textarea
            value={form.config.body ?? ''}
            onChange={(e) => setCfg({ body: e.target.value })}
            rows={3}
            placeholder='{"name": "{name}", "role": "admin"}'
            className={`${INPUT_CLS} resize-y`}
          />

          <label className={label}>parameters (what the agent can pass)</label>
          <div className="grid gap-2">
            {(form.config.params ?? []).map((p, i) => (
              <div key={i} className="rounded-lg border border-border bg-background p-2.5">
                <div className="flex flex-wrap items-center gap-1.5">
                  <input value={p.name} onChange={(e) => { const ps = [...(form.config.params ?? [])]; ps[i] = { ...ps[i], name: e.target.value }; setParams(ps); }} placeholder="paramName" className={`${INPUT_CLS} min-w-[120px] flex-1`} />
                  <div className="w-28 shrink-0">
                    <Select value={p.type} options={PARAM_TYPES.map((t) => ({ value: t, label: t }))} onChange={(v) => { const ps = [...(form.config.params ?? [])]; ps[i] = { ...ps[i], type: v }; setParams(ps); }} />
                  </div>
                  <IconButton label="remove parameter" onClick={() => setParams((form.config.params ?? []).filter((_, j) => j !== i))}><Trash2 size={12} /></IconButton>
                </div>
                <input value={p.description} onChange={(e) => { const ps = [...(form.config.params ?? [])]; ps[i] = { ...ps[i], description: e.target.value }; setParams(ps); }} placeholder="What is this param? The LLM uses this to fill it." className={`${INPUT_CLS} mt-1.5`} />
                <div className="-mx-2 mt-1">
                  <Checkbox
                    checked={p.required}
                    onChange={(next) => { const ps = [...(form.config.params ?? [])]; ps[i] = { ...ps[i], required: next }; setParams(ps); }}
                    label="required"
                  />
                </div>
              </div>
            ))}
            <Button className={addBtn} icon={<Plus size={12} />} onClick={() => setParams([...(form.config.params ?? []), { name: '', type: 'string', description: '', required: false }])}>add parameter</Button>
          </div>
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
