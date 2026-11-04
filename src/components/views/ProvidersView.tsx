import { useState } from 'react';
import { Plus } from 'lucide-react';
import type { ProviderCaps, ProviderRecord } from '../../types';
import type { ProviderDraft as DraftFromRuntime } from '../../runtime';
import { useProvidersStore } from '../../hooks/useProviders';
import { toast } from '../../hooks/useToast';
import { Modal } from '../ui/Modal';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { Checkbox } from '../ui/Checkbox';
import { Card } from '../ui/Card';
import { ModelLogo } from '../ui/ModelLogo';
import { FIELD_LABEL_CLS, INPUT_CLS } from '../ui/Input';

type Draft = DraftFromRuntime;

const emptyCaps = (): ProviderCaps => ({ chat: true, vision: false, embeddings: false, transcription: false, speech: false });

const emptyProvider = (): Draft => ({
  id: '', label: '', baseUrl: '', auth: 'bearer', authHeader: 'x-api-key',
  extraHeaders: {}, requiresKey: true, reasoning: 'openai', providerRouting: false,
  caps: emptyCaps(), hasKey: false, builtin: false, apiKey: '', clearKey: false,
});

const CAP_LABEL: { key: keyof ProviderCaps; label: string }[] = [
  { key: 'vision', label: 'vision — image input' },
  { key: 'embeddings', label: 'embeddings — vectors for recall' },
  { key: 'transcription', label: 'transcription — audio to text' },
  { key: 'speech', label: 'speech — text to audio' },
];

const capsSummary = (c: ProviderCaps) => {
  const on = CAP_LABEL.filter((k) => c[k.key]).map((k) => k.key);
  return ['chat', ...on].join(' · ');
};

export function ProvidersView() {
  const providers = useProvidersStore((s) => s.providers);
  const deleteProvider = useProvidersStore((s) => s.deleteProvider);
  const [editing, setEditing] = useState<Draft | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [keyFor, setKeyFor] = useState<ProviderRecord | null>(null);

  return (
    <>
      <div className="mb-4 flex items-center justify-between gap-3">
        <p className="m-0 max-w-[520px] font-mono text-[10px] leading-relaxed text-muted">
          Providers are adapters: an endpoint, a wire dialect, auth and capabilities. Built-ins are fixed;
          add any OpenAI-compatible server (LM Studio, llama.cpp, DeepSeek, Together, Azure…) here and its
          models become available everywhere.
        </p>
        <Button variant="primary" icon={<Plus size={13} />} onClick={() => setEditing(emptyProvider())}>add provider</Button>
      </div>

      <div className="grid gap-2">
        {providers.map((p) => (
          <Card key={p.id} className="flex items-center justify-between gap-4 hover:bg-surface">
            <div className="flex min-w-0 items-center gap-2.5">
              <ModelLogo model={{ provider: p.id, model: '', host: p.baseUrl }} size={16} />
              <div className="min-w-0">
                <b className="block truncate font-mono text-[11px] text-foreground">
                  {p.label} <span className="font-normal text-muted">· {p.id}</span>
                  {!p.builtin && <span className="font-normal text-muted"> · custom</span>}
                </b>
                <span className="block truncate font-mono text-[10px] text-muted">
                  {p.baseUrl}{p.hasKey ? ' · key set' : ''}{p.auth !== 'bearer' ? ` · ${p.auth}` : ''}
                </span>
                <span className="block truncate font-mono text-[10px] text-muted/70">{capsSummary(p.caps)}</span>
              </div>
            </div>
            {p.builtin ? (
              <Button className="shrink-0" onClick={() => setKeyFor(p)}>{p.hasKey ? 'update key' : 'set key'}</Button>
            ) : (
              <div className="flex shrink-0 items-center gap-2">
                <Button onClick={() => setEditing({ ...p, apiKey: '', clearKey: false })}>edit</Button>
                <Button onClick={() => setConfirming(p.id)}>delete</Button>
              </div>
            )}
          </Card>
        ))}
        {providers.length === 0 && (
          <p className="m-0 font-mono text-[11px] text-muted">providers not loaded — restart the app</p>
        )}
      </div>

      {editing && (
        <ProviderForm
          editing={editing}
          onClose={() => setEditing(null)}
          onSave={async (p) => { await useProvidersStore.getState().saveProvider(p); setEditing(null); toast('provider saved', 'success'); }}
        />
      )}

      {keyFor && (
        <ProviderKeyForm
          provider={keyFor}
          onClose={() => setKeyFor(null)}
          onSave={async (apiKey, clearKey) => {
            await useProvidersStore.getState().saveProvider({ ...keyFor, apiKey, clearKey });
            setKeyFor(null);
            toast('provider key saved', 'success');
          }}
        />
      )}

      {confirming && (
        <Modal
          title={`delete ${confirming}`}
          onClose={() => setConfirming(null)}
          headerAction={<Button variant="primary" onClick={async () => { await deleteProvider(confirming); setConfirming(null); toast('provider deleted', 'success'); }}>delete</Button>}
        >
          <p className="m-0 text-[12px] leading-relaxed text-muted">
            This removes the adapter and its stored key. Models using <span className="font-mono">“{confirming}:…”</span> will
            stop resolving until you re-add it or point them at another provider.
          </p>
        </Modal>
      )}
    </>
  );
}

function ProviderForm({ editing, onClose, onSave }: {
  editing: Draft; onClose: () => void; onSave: (p: Draft) => Promise<void>;
}) {
  const [form, setForm] = useState<Draft>(editing);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const isNew = !editing.id;
  const setCap = (key: keyof ProviderCaps, value: boolean) =>
    setForm((f) => ({ ...f, caps: { ...f.caps, chat: true, [key]: value } }));

  const save = async () => {
    const id = form.id.trim().toLowerCase();
    if (!id) { setError('The provider id is required.'); return; }
    if (id.includes(':') || !/^[a-z0-9._-]+$/.test(id)) { setError('Use lowercase letters, digits, dot, dash or underscore — no “:”.'); return; }
    if (!form.label.trim()) { setError('A label is required.'); return; }
    if (!form.baseUrl.trim()) { setError('The base URL is required (e.g. https://api.deepseek.com/v1).'); return; }
    setSaving(true); setError(undefined);
    try {
      await onSave({
        ...form, id, label: form.label.trim(), baseUrl: form.baseUrl.trim().replace(/\/+$/, ''),
        authHeader: form.auth === 'x-api-key' ? (form.authHeader.trim() || 'x-api-key') : '',
        apiKey: form.apiKey?.trim() || undefined,
        clearKey: form.clearKey || undefined,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={isNew ? 'add provider' : `edit ${form.label}`}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
    >
      <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
        Custom providers speak the OpenAI API. The base URL is the API root — requests go to
        <span className="text-foreground"> /chat/completions</span>, and to
        <span className="text-foreground"> /embeddings</span>, <span className="text-foreground">/audio/transcriptions</span> or
        <span className="text-foreground"> /audio/speech</span> for the capabilities you tick.
      </p>

      <label className={FIELD_LABEL_CLS}>id</label>
      <input
        value={form.id}
        disabled={!isNew}
        onChange={(e) => setForm({ ...form, id: e.target.value })}
        placeholder="e.g. deepseek, lmstudio"
        className={`${INPUT_CLS} ${!isNew ? 'opacity-60' : ''}`}
      />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>label</label>
      <input value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} placeholder="e.g. DeepSeek" className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>base url</label>
      <input value={form.baseUrl} onChange={(e) => setForm({ ...form, baseUrl: e.target.value })} placeholder="https://api.deepseek.com/v1" className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>auth</label>
      <Select
        value={form.auth}
        options={[
          { value: 'bearer', label: 'bearer — Authorization: Bearer <key>' },
          { value: 'x-api-key', label: 'api-key header' },
          { value: 'none', label: 'none — local server, no key' },
        ]}
        onChange={(v) => setForm({ ...form, auth: v })}
      />
      {form.auth === 'x-api-key' && (
        <>
          <label className={`${FIELD_LABEL_CLS} mt-4`}>header name</label>
          <input value={form.authHeader} onChange={(e) => setForm({ ...form, authHeader: e.target.value })} placeholder="x-api-key" className={INPUT_CLS} />
        </>
      )}

      {form.auth !== 'none' && (
        <>
          <label className={`${FIELD_LABEL_CLS} mt-4`}>api key</label>
          <input
            type="password"
            value={form.apiKey ?? ''}
            onChange={(e) => setForm({ ...form, apiKey: e.target.value, clearKey: false })}
            placeholder={editing.hasKey && !form.clearKey ? '•••••••• saved — type to replace' : 'shared by this provider’s models'}
            className={INPUT_CLS}
          />
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
            {form.clearKey ? (
              <>Key will be removed on save. <button type="button" className="cursor-pointer underline" onClick={() => setForm({ ...form, clearKey: false })}>undo</button></>
            ) : editing.hasKey ? (
              <>Stored in the system credential store. A model can still carry its own key to override it. <button type="button" className="cursor-pointer underline" onClick={() => setForm({ ...form, clearKey: true })}>remove key</button></>
            ) : (
              'Optional here: a model without its own key inherits this one.'
            )}
          </p>
        </>
      )}

      <label className={`${FIELD_LABEL_CLS} mt-4`}>capabilities</label>
      <div className="grid gap-2">
        <Checkbox checked disabled onChange={() => {}} label="chat — always on" />
        {CAP_LABEL.map((c) => (
          <Checkbox key={c.key} checked={!!form.caps[c.key]} onChange={(v) => setCap(c.key, v)} label={c.label} />
        ))}
      </div>

      <label className={`${FIELD_LABEL_CLS} mt-4`}>reasoning style</label>
      <Select
        value={form.reasoning}
        options={[
          { value: 'openai', label: 'openai — reasoning_effort' },
          { value: 'openrouter', label: 'openrouter — reasoning.effort' },
          { value: 'none', label: 'none — never send it' },
        ]}
        onChange={(v) => setForm({ ...form, reasoning: v })}
      />

      <div className="-mx-2 mt-3.5 grid gap-2">
        <Checkbox checked={form.requiresKey} onChange={(v) => setForm({ ...form, requiresKey: v })} label="requires an API key" />
        <Checkbox checked={form.providerRouting} onChange={(v) => setForm({ ...form, providerRouting: v })} label="route with a throughput hint (provider: { sort: 'throughput' })" />
      </div>

      {error && <p className="mt-2.5 mb-0 font-mono text-[10px] text-danger">{error}</p>}
    </Modal>
  );
}

// A built-in adapter has no editable config — only its shared key. This is the
// `provider/<id>` key a model without its own key falls back to, so without it a
// built-in like OpenRouter could never satisfy `requires_key`.
function ProviderKeyForm({ provider, onClose, onSave }: {
  provider: ProviderRecord; onClose: () => void; onSave: (apiKey?: string, clearKey?: boolean) => Promise<void>;
}) {
  const [key, setKey] = useState('');
  const [clear, setClear] = useState(false);
  const [saving, setSaving] = useState(false);
  const submit = async () => {
    setSaving(true);
    try { await onSave(key.trim() || undefined, clear || undefined); }
    finally { setSaving(false); }
  };
  return (
    <Modal
      title={`${provider.label} — api key`}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={saving} onClick={submit}>{saving ? 'saving…' : 'save'}</Button>}
    >
      <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
        The shared key for {provider.label}. A model on this provider without its own key uses it.
        Stored in the system credential store and never sent to the window.
      </p>
      <label className={FIELD_LABEL_CLS}>api key</label>
      <input
        type="password"
        value={key}
        onChange={(e) => { setKey(e.target.value); setClear(false); }}
        placeholder={provider.hasKey && !clear ? '•••••••• saved — type to replace' : 'paste the api key'}
        className={INPUT_CLS}
      />
      <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
        {clear ? (
          <>Key will be removed on save. <button type="button" className="cursor-pointer underline" onClick={() => setClear(false)}>undo</button></>
        ) : provider.hasKey ? (
          <>A key is stored. <button type="button" className="cursor-pointer underline" onClick={() => setClear(true)}>remove key</button></>
        ) : (
          'No key set — models on this provider fail until you add one.'
        )}
      </p>
    </Modal>
  );
}
