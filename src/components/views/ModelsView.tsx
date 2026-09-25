import { useState } from 'react';
import { Check } from 'lucide-react';
import type { ModelConfig, Reasoning } from '../../types';
import { Modal } from '../ui/Modal';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { Checkbox } from '../ui/Checkbox';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, INPUT_CLS } from '../ui/Input';

export function ModelsView({ models, onAdd, onEdit, onDelete }: {
  models: ModelConfig[]; onAdd: () => void; onEdit: (m: ModelConfig) => void; onDelete: (id: string) => Promise<void>;
}) {
  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button variant="primary" icon={<Check size={13} />} onClick={onAdd}>add model</Button>
      </div>

      <div className="grid gap-2">
        {models.map((m) => (
          <Card key={m.id} className="flex items-center justify-between gap-4 hover:bg-surface">
            <div className="min-w-0">
              <b className="block truncate font-mono text-[11px] text-foreground">{m.label}</b>
              <span className="block truncate font-mono text-[10px] text-muted">
                {m.provider}:{m.model}{m.host ? ` · ${m.host}` : ''}{m.apiKey ? ' · key set' : ''}{m.reasoning !== 'auto' ? ` · reasoning: ${m.reasoning}` : ''}
              </span>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <span className="flex items-center gap-1.5 font-mono text-[10px] text-muted">
                <i className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${m.enabled ? 'bg-foreground' : 'bg-muted'}`} />
                {m.enabled ? 'enabled' : 'disabled'}
              </span>
              <Button onClick={() => onEdit(m)}>edit</Button>
              <Button onClick={() => onDelete(m.id)}>delete</Button>
            </div>
          </Card>
        ))}
        {models.length === 0 && <p className="m-0 font-mono text-[11px] text-muted">no models configured — add one to get started</p>}
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

  return (
    <Modal
      title={isNew ? 'add model' : `edit ${form.label}`}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
    >
      <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">API keys are stored locally in the app's SQLite database — they never leave your machine.</p>

      <label className={FIELD_LABEL_CLS}>provider</label>
      <Select
        value={form.provider}
        options={[{ value: 'ollama', label: 'Ollama (local)' }, { value: 'openrouter', label: 'OpenRouter' }, { value: 'groq', label: 'Groq' }]}
        onChange={(v) => setForm({ ...form, provider: v as 'ollama' | 'openrouter' | 'groq' })}
      />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>label</label>
      <input value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} placeholder="e.g. Qwen3 8B" className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>model id</label>
      <input value={form.model} onChange={(e) => setForm({ ...form, model: e.target.value })} placeholder={form.provider === 'ollama' ? 'qwen3:8b' : form.provider === 'groq' ? 'llama-3.3-70b-versatile' : 'anthropic/claude-3.5-haiku'} className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>reasoning</label>
      <Select
        value={form.reasoning}
        options={[
          { value: 'auto', label: 'auto — leave it to the model' },
          { value: 'off', label: 'off — answer directly' },
          { value: 'low', label: 'low effort' },
          { value: 'medium', label: 'medium effort' },
          { value: 'high', label: 'high effort' },
        ]}
        onChange={(v) => setForm({ ...form, reasoning: v as Reasoning })}
      />
      <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
        Thinking models can spend thousands of tokens before answering, and those count against the reply's own budget. Auto sends nothing,
        which is the right choice for models that don't support reasoning.
      </p>

      {form.provider === 'ollama' && (
        <>
          <label className={`${FIELD_LABEL_CLS} mt-4`}>host</label>
          <input value={form.host ?? ''} onChange={(e) => setForm({ ...form, host: e.target.value })} placeholder="http://localhost:11434" className={INPUT_CLS} />
        </>
      )}

      {(form.provider === 'openrouter' || form.provider === 'groq') && (
        <>
          <label className={`${FIELD_LABEL_CLS} mt-4`}>api key</label>
          <input type="password" value={form.apiKey ?? ''} onChange={(e) => setForm({ ...form, apiKey: e.target.value })} placeholder={form.provider === 'groq' ? 'gsk_…' : 'sk-or-v1-…'} className={INPUT_CLS} />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">Runs using this model will read the key automatically.</p>
        </>
      )}

      <div className="-mx-2 mt-3.5">
        <Checkbox checked={form.enabled} onChange={(next) => setForm({ ...form, enabled: next })} label="enabled" hint="available to agents" />
      </div>

      {error && <p className="mt-2.5 mb-0 font-mono text-[10px] text-danger">{error}</p>}
    </Modal>
  );
}
