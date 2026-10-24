import { useEffect, useState } from 'react';
import { Check } from 'lucide-react';
import type { ModelConfig, ModelRoles, Provider, Reasoning } from '../../types';
import { listModelRoles, setModelRole } from '../../runtime';
import { Modal } from '../ui/Modal';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { Slider } from '../ui/Slider';
import { REASONING_STOPS } from '../../reasoning';
import { Checkbox } from '../ui/Checkbox';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, INPUT_CLS } from '../ui/Input';

// Example ids and key shapes per provider, so the form shows what to type.
const MODEL_PLACEHOLDER: Record<string, string> = {
  ollama: 'qwen3:8b',
  openai: 'gpt-4o-mini',
  google: 'gemini-2.0-flash',
  xai: 'grok-3',
  anthropic: 'claude-sonnet-4-5',
  groq: 'llama-3.3-70b-versatile',
  openrouter: 'anthropic/claude-3.5-haiku',
};
// What each model role is for, shown beside the picker in the order they matter.
const ROLE_ORDER = ['classifier', 'embedding', 'reranker', 'vision', 'distillation'];
const ROLE_HINT: Record<string, string> = {
  classifier: 'reads captures into structured data',
  embedding: 'vectorises text for semantic recall',
  reranker: 'reorders retrieved memory',
  vision: 'sees dropped images and screenshots',
  distillation: 'turns finished chats into memory',
};
const KEY_PLACEHOLDER: Record<string, string> = {
  openai: 'sk-…',
  google: 'AIza…',
  xai: 'xai-…',
  anthropic: 'sk-ant-…',
  groq: 'gsk_…',
  openrouter: 'sk-or-v1-…',
};

export function ModelsView({ models, onAdd, onEdit, onDelete }: {
  models: ModelConfig[]; onAdd: () => void; onEdit: (m: ModelConfig) => void; onDelete: (id: string) => Promise<void>;
}) {
  // Roles: which model fills classifier / embedding. Unset means "auto" (a
  // deterministic default in the backend), so nothing here is required.
  const [roles, setRoles] = useState<ModelRoles>({});
  useEffect(() => { void listModelRoles().then(setRoles).catch(() => setRoles({})); }, []);
  const assign = async (role: string, modelId: string) => {
    await setModelRole(role, modelId);
    setRoles(await listModelRoles().catch(() => ({})));
  };
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
                {m.provider}:{m.model}{m.host ? ` · ${m.host}` : ''}{m.hasKey ? ' · key set' : ''}{m.vision ? ' · vision' : ''}{m.reasoning !== 'auto' ? ` · reasoning: ${m.reasoning}` : ''}
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

      <Card className="mt-4">
        <b className="block font-mono text-[11px] text-foreground">model roles</b>
        <p className="mt-1 mb-3 font-mono text-[10px] leading-relaxed text-muted">
          One model need not do every job. Pin which model reads your captures, which vectorises text for recall, which reorders
          retrieved memory, which describes dropped images, and which distils chats into memory. Models that can see are marked
          vision; leave a role on auto and the app picks a default.
        </p>
        <div className="grid gap-3 sm:grid-cols-2">
          {ROLE_ORDER.filter((role) => role in roles).map((role) => {
            // For vision the models that can actually see come first and are
            // marked — pinning a text-only model there would describe nothing.
            const options = [
              { value: '', label: role === 'vision' ? 'off' : 'auto' },
              ...[...models]
                .sort((a, b) => (role === 'vision' ? Number(b.vision ?? false) - Number(a.vision ?? false) : 0))
                .map((m) => ({ value: m.id, label: `${m.label || `${m.provider}:${m.model}`}${m.vision ? ' · vision' : ''}` })),
            ];
            return (
              <div key={role}>
                <label className={FIELD_LABEL_CLS}>{role}</label>
                <Select
                  value={roles[role]?.modelId ?? ''}
                  options={options}
                  onChange={(v) => void assign(role, v)}
                  placeholder={role === 'vision' ? 'off' : 'auto'}
                />
                <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
                  {ROLE_HINT[role] ?? ''}
                  {' — '}
                  {roles[role]?.resolved
                    ? `using ${roles[role]?.resolved}`
                    : role === 'vision'
                      ? 'off — images go straight to the chat model'
                      : (role === 'classifier' || role === 'embedding' ? 'no model available' : 'not set — off until you pick one')}
                </p>
              </div>
            );
          })}
        </div>
      </Card>
    </>
  );
}

export function ModelFormDrawer({ editing, isNew, onClose, onSave }: {
  editing: ModelConfig; isNew: boolean; onClose: () => void; onSave: (m: ModelConfig) => Promise<void>;
}) {
  const [form, setForm] = useState<ModelConfig>(editing);
  const [saving, setSaving] = useState(false);
  const [clearKey, setClearKey] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);

  const save = async () => {
    if (!form.label.trim() || !form.model.trim()) { setError('Label and model name are required.'); return; }
    const id = form.id || `${form.provider}:${form.model}`;
    setSaving(true); setError(undefined);
    try {
      await onSave({
        ...form, id, host: form.host?.trim() || undefined,
        apiKey: form.apiKey?.trim() || undefined,
        clearKey: clearKey || undefined,
      });
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
        options={[
          { value: 'ollama', label: 'Ollama (local)' },
          { value: 'openai', label: 'OpenAI' },
          { value: 'anthropic', label: 'Anthropic' },
          { value: 'google', label: 'Google (Gemini)' },
          { value: 'xai', label: 'xAI (Grok)' },
          { value: 'groq', label: 'Groq' },
          { value: 'openrouter', label: 'OpenRouter' },
        ]}
        onChange={(v) => setForm({ ...form, provider: v as Provider })}
      />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>label</label>
      <input value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} placeholder="e.g. Qwen3 8B" className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>model id</label>
      <input value={form.model} onChange={(e) => setForm({ ...form, model: e.target.value })} placeholder={MODEL_PLACEHOLDER[form.provider] ?? ''} className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>reasoning</label>
      <Slider
        value={form.reasoning}
        stops={REASONING_STOPS}
        ariaLabel="reasoning"
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

      {form.provider !== 'ollama' && (
        <>
          <label className={`${FIELD_LABEL_CLS} mt-4`}>api key</label>
          <input
            type="password"
            value={form.apiKey ?? ''}
            onChange={(e) => { setClearKey(false); setForm({ ...form, apiKey: e.target.value }); }}
            placeholder={editing.hasKey && !clearKey ? '•••••••• saved — type to replace' : (KEY_PLACEHOLDER[form.provider] ?? '')}
            className={INPUT_CLS}
          />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
            {clearKey ? (
              <>Key will be removed on save. <button type="button" className="cursor-pointer underline" onClick={() => setClearKey(false)}>undo</button></>
            ) : editing.hasKey ? (
              <>Kept in your system credential store, never in the app database. <button type="button" className="cursor-pointer underline" onClick={() => setClearKey(true)}>remove key</button></>
            ) : (
              'Saved to your system credential store; runs read it automatically.'
            )}
          </p>
        </>
      )}

      <div className="-mx-2 mt-3.5">
        <Checkbox checked={form.enabled} onChange={(next) => setForm({ ...form, enabled: next })} label="enabled" hint="available to agents" />
      </div>

      {error && <p className="mt-2.5 mb-0 font-mono text-[10px] text-danger">{error}</p>}
    </Modal>
  );
}
