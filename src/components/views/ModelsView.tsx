import { useState } from 'react';
import { Check } from 'lucide-react';
import type { ModelConfig } from '../../types';
import { Drawer } from '../ui/Drawer';
import { Dropdown } from '../ui/Dropdown';

export function ModelsView({ models, onAdd, onEdit, onDelete, embedded = false }: {
  models: ModelConfig[]; onAdd: () => void; onEdit: (m: ModelConfig) => void; onDelete: (id: string) => Promise<void>; embedded?: boolean;
}) {
  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        {!embedded && <div><span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span><h1 style={{ margin: 0, fontSize: 24 }}>Models</h1></div>}
        <button className="primary ml-auto" onClick={onAdd}><Check size={13} />Add model</button>
      </header>
      <div className="model-config max-w-[760px] rounded-[10px] border border-line bg-panel p-[22px]">
        {models.map((m) => (
          <div key={m.id} style={{ padding: '12px 0', borderBottom: '1px solid var(--line)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <b style={{ fontSize: 13 }}>{m.label}</b>
                <span style={{ display: 'block', fontSize: 11, color: 'var(--muted)' }}>{m.provider}:{m.model}{m.host ? ` · ${m.host}` : ''}{m.apiKey ? ' · key set' : ''}</span>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span className="mr-[7px] text-[10px] text-muted"><i className={`mr-1.5 inline-block h-[7px] w-[7px] rounded-full ${m.enabled ? 'bg-[var(--green)]' : 'bg-[#f79009]'}`} />{m.enabled ? 'enabled' : 'disabled'}</span>
                <button className="secondary" onClick={() => onEdit(m)}>Edit</button>
                <button className="secondary" onClick={() => onDelete(m.id)}>Delete</button>
              </div>
            </div>
          </div>
        ))}
        {models.length === 0 && <p className="mt-[22px] text-[12px] text-muted">No models configured. Add one to get started.</p>}
      </div>
    </>
  );
}

export function ModelFormDrawer({ editing, isNew, onClose, onSave }: {
  editing: ModelConfig; isNew: boolean; onClose: () => void; onSave: (m: ModelConfig) => Promise<void>;
}) {
  const [form, setForm] = useState<ModelConfig>(editing);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);

  const save = async () => {
    if (!form.label.trim() || !form.model.trim()) { setError('Label and model name are required.'); return; }
    const id = form.id || `${form.provider}:${form.model}`;
    setSaving(true); setError(undefined);
    try {
      await onSave({ ...form, id, host: form.host?.trim() || undefined, apiKey: form.apiKey?.trim() || undefined });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save model config.');
    } finally {
      setSaving(false);
    }
  };

  const inputCls = 'w-full rounded-[6px] border border-line bg-panel2 px-3 py-2.5 text-text';
  const labelCls = 'mt-4 mb-1.5 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase';

  return (
    <Drawer
      title={isNew ? 'Add model' : `Edit ${form.label}`}
      onClose={onClose}
      initialWidth={Math.round(window.innerWidth / 2)}
      resizable
      headerAction={<button className="primary" disabled={saving} onClick={save}><Check size={13} />{saving ? 'Saving…' : 'Save'}</button>}
    >
      <div className="w-full rounded-[10px] border border-line bg-panel p-[22px]">
        <p className="text-[12px] leading-[1.6] text-muted" style={{ margin: '0 0 14px' }}>API keys are stored locally in the app's SQLite database — they never leave your machine.</p>

        <label className={labelCls}>PROVIDER</label>
        <Dropdown
          value={form.provider}
          options={[{ value: 'ollama', label: 'Ollama (local)' }, { value: 'openrouter', label: 'OpenRouter' }, { value: 'groq', label: 'Groq' }]}
          onChange={(v) => setForm({ ...form, provider: v as 'ollama' | 'openrouter' | 'groq' })}
        />

        <label className={labelCls}>LABEL</label>
        <input value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} placeholder="e.g. Qwen3 8B" className={inputCls} />

        <label className={labelCls}>MODEL ID</label>
        <input value={form.model} onChange={(e) => setForm({ ...form, model: e.target.value })} placeholder={form.provider === 'ollama' ? 'qwen3:8b' : form.provider === 'groq' ? 'llama-3.3-70b-versatile' : 'anthropic/claude-3.5-haiku'} className={inputCls} />

        {form.provider === 'ollama' && (
          <>
            <label className={labelCls}>HOST</label>
            <input value={form.host ?? ''} onChange={(e) => setForm({ ...form, host: e.target.value })} placeholder="http://localhost:11434" className={inputCls} />
          </>
        )}

        {(form.provider === 'openrouter' || form.provider === 'groq') && (
          <>
            <label className={labelCls}>API KEY</label>
            <input type="password" value={form.apiKey ?? ''} onChange={(e) => setForm({ ...form, apiKey: e.target.value })} placeholder={form.provider === 'groq' ? 'gsk_…' : 'sk-or-v1-…'} className={inputCls} />
            <p className="text-[12px] text-muted" style={{ margin: '6px 0 0' }}>Runs using this model will read the key automatically.</p>
          </>
        )}

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
