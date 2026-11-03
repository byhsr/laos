import { useEffect, useState } from 'react';
import { Check } from 'lucide-react';
import type { ModelConfig, ModelRoles, ModelType, Reasoning } from '../../types';
import { MODEL_TYPES } from '../../modelTypes';
import { listModelRoles, setModelRole } from '../../runtime';
import { useProvidersStore } from '../../hooks/useProviders';
import { ModelLogo } from '../ui/ModelLogo';
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
const ROLE_ORDER = ['classifier', 'embedding', 'reranker', 'vision', 'transcriber', 'distillation'];
const ROLE_HINT: Record<string, string> = {
  classifier: 'reads captures into structured data',
  embedding: 'vectorises text for semantic recall',
  reranker: 'reorders retrieved memory',
  vision: 'sees dropped images and screenshots',
  transcriber: 'turns voice notes and audio into text',
  distillation: 'turns finished chats into memory',
};
// Shown before the provider list has loaded (or in the browser preview).
const FALLBACK_PROVIDERS = [
  { value: 'ollama', label: 'Ollama (local)' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'google', label: 'Google (Gemini)' },
  { value: 'xai', label: 'xAI (Grok)' },
  { value: 'groq', label: 'Groq' },
  { value: 'openrouter', label: 'OpenRouter' },
];
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
            <div className="flex min-w-0 items-center gap-2.5">
              <ModelLogo model={m} size={16} />
              <div className="min-w-0">
                <b className="block truncate font-mono text-[11px] text-foreground">
                  {m.label} <span className="font-normal text-muted">· {m.kind ?? 'llm'}</span>
                </b>
                <span className="block truncate font-mono text-[10px] text-muted">
                  {m.provider}:{m.model}{m.host ? ` · ${m.host}` : ''}{m.hasKey ? ' · key set' : ''}{m.reasoning !== 'auto' ? ` · reasoning: ${m.reasoning}` : ''}
                </span>
              </div>
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
          retrieved memory, which describes dropped images, which transcribes voice notes and audio, and which distils chats into
          memory. Models that can see are marked vision, transcribing providers audio; leave a role on auto and the app picks a default.
        </p>
        <div className="grid gap-3 sm:grid-cols-2">
          {ROLE_ORDER.filter((role) => role in roles).map((role) => {
            // Capability roles want a model that can actually do the job — a
            // vision model for images, a transcribing provider for audio. Those
            // come first and are marked, so pinning the wrong kind is obvious.
            const can = (m: ModelConfig) => {
              const kind = m.kind ?? 'llm';
              return role === 'vision'
                ? !!m.vision || kind === 'vision' || kind === 'multimedia'
                : role === 'transcriber'
                  ? !!m.transcribes || kind === 'stt'
                  : false;
            };
            const off = role === 'vision' || role === 'transcriber';
            const options = [
              { value: '', label: off ? 'off' : 'auto' },
              ...[...models]
                .sort((a, b) => Number(can(b)) - Number(can(a)))
                .map((m) => {
                  const suffix = can(m) ? (role === 'vision' ? ' · vision' : ' · audio') : '';
                  return { value: m.id, label: `${m.label || `${m.provider}:${m.model}`}${suffix}` };
                }),
            ];
            const offText = role === 'vision' ? 'off — images go straight to the chat model' : 'off — audio is left untranscribed';
            return (
              <div key={role}>
                <label className={FIELD_LABEL_CLS}>{role}</label>
                <Select
                  value={roles[role]?.modelId ?? ''}
                  options={options}
                  onChange={(v) => void assign(role, v)}
                  placeholder={off ? 'off' : 'auto'}
                />
                <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
                  {ROLE_HINT[role] ?? ''}
                  {' — '}
                  {roles[role]?.resolved
                    ? `using ${roles[role]?.resolved}`
                    : off
                      ? offText
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
  const providers = useProvidersStore((s) => s.providers);
  const providerOptions = providers.length
    ? providers.map((p) => ({ value: p.id, label: p.label + (p.builtin ? '' : ' · custom') }))
    : FALLBACK_PROVIDERS;
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

      {/* The logo is resolved from the provider (and host for Ollama), so it shows
          here the moment it will show in the list. */}
      <div className="mb-3.5 flex items-center gap-2.5 rounded-lg border border-border bg-surface px-2.5 py-2">
        <ModelLogo model={form} size={18} />
        <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{form.label || form.model || 'new model'}</span>
        <span className="shrink-0 font-mono text-[10px] text-muted">{form.kind ?? 'llm'}</span>
      </div>

      <label className={FIELD_LABEL_CLS}>provider</label>
      <Select
        value={form.provider}
        options={providerOptions}
        onChange={(v) => setForm({ ...form, provider: v })}
      />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>type</label>
      <Select
        value={form.kind ?? 'llm'}
        options={MODEL_TYPES.map((t) => ({ value: t.value, label: `${t.label} — ${t.hint}` }))}
        onChange={(v) => setForm({ ...form, kind: v as ModelType })}
      />
      <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
        What the model is for. This groups it in the chat picker and tells the role pickers which models can do a job.
      </p>

      <label className={`${FIELD_LABEL_CLS} mt-4`}>label</label>
      <input value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} placeholder="e.g. Qwen3 8B" className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>model id</label>
      <input value={form.model} onChange={(e) => setForm({ ...form, model: e.target.value })} placeholder={MODEL_PLACEHOLDER[form.provider] ?? 'model id'} className={INPUT_CLS} />

      {(form.kind ?? 'llm') === 'tts' && (
        <>
          <label className={`${FIELD_LABEL_CLS} mt-4`}>voice</label>
          <input value={form.voice ?? ''} onChange={(e) => setForm({ ...form, voice: e.target.value })} placeholder="provider voice id (alloy, Kore, en-US-Harper:MAI-Voice-2.1…)" className={INPUT_CLS} />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
            Voices are model-specific. Leave blank to fall back to the global voice in Settings → voice.
          </p>
        </>
      )}

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
