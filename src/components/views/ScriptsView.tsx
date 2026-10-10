import { useState } from 'react';
import { Check, Play, Terminal, Trash2 } from 'lucide-react';
import type { ModelConfig, Script } from '../../types';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { Select } from '../ui/Select';
import { FIELD_LABEL_CLS, INPUT_CLS, PROSE_CLS } from '../ui/Input';
import { runScriptNow } from '../../runtime';
import { toast } from '../../hooks/useToast';

export function ScriptsView({ scripts, onAdd, onEdit, onDelete }: {
  scripts: Script[]; onAdd: () => void; onEdit: (s: Script) => void; onDelete: (id: string) => Promise<void>;
}) {
  const [running, setRunning] = useState<string | null>(null);

  const run = async (s: Script) => {
    setRunning(s.id);
    try {
      const out = await runScriptNow(s.id);
      toast(out.trim() ? out.slice(0, 400) : 'ran with no output', 'success');
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      setRunning(null);
    }
  };

  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button variant="primary" icon={<Check size={13} />} onClick={onAdd}>add script</Button>
      </div>

      <div className="grid gap-2">
        {scripts.map((s) => (
          <Card key={s.id} className="flex items-center gap-4 hover:bg-surface">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-muted"><Terminal size={16} /></span>
            <div className="min-w-0 flex-1">
              <b className="block truncate font-mono text-[11px] text-foreground">{s.name}</b>
              <span className="block truncate font-mono text-[10px] text-muted">{s.description || 'no description'}</span>
              <span className="mt-0.5 block truncate font-mono text-[10px] text-muted">{s.kind === 'prompt' ? `prompt · ${s.prompt.slice(0, 64) || 'empty'}` : `command · ${s.command}`}</span>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              <IconButton label="run" disabled={running === s.id} onClick={() => void run(s)}><Play size={12} /></IconButton>
              <Button onClick={() => onEdit(s)}>edit</Button>
              <IconButton label="delete script" onClick={() => void onDelete(s.id)}><Trash2 size={12} /></IconButton>
            </div>
          </Card>
        ))}
        {scripts.length === 0 && (
          <p className="m-0 font-mono text-[11px] text-muted">no scripts yet — add a reusable command, then run it from here, from an agent with host file access, or from a workflow.</p>
        )}
      </div>
    </>
  );
}

export function ScriptFormDrawer({ editing, isNew, models, onClose, onSave }: {
  editing: Script; isNew: boolean; models: ModelConfig[]; onClose: () => void; onSave: (s: Script) => Promise<void>;
}) {
  const [form, setForm] = useState<Script>(editing);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const [testOut, setTestOut] = useState<string | null>(null);

  const save = async () => {
    if (!form.name.trim()) { setError('App needs a name.'); return; }
    if (form.kind === 'prompt') {
      if (!form.prompt.trim()) { setError('A prompt app needs a prompt.'); return; }
      if (!form.model.trim()) { setError('A prompt app needs a model.'); return; }
    } else if (!form.command.trim()) {
      setError('A command app needs a command.'); return;
    }
    setSaving(true); setError(undefined);
    try {
      await onSave({ ...form, name: form.name.trim(), description: form.description.trim() });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save app.');
    } finally {
      setSaving(false);
    }
  };

  const test = async () => {
    if (!form.id) { setError('Save the app before testing it.'); return; }
    setError(undefined);
    try {
      setTestOut(await runScriptNow(form.id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <Modal
      title={isNew ? 'add app' : `edit ${form.name}`}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
    >
      <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
        A custom app runs inside the app. A <b>command</b> app runs a shell command; a <b>prompt</b> app runs a single model call. Use <code className="font-mono">{'{input}'}</code> where the caller's text should go.
      </p>

      <label className={FIELD_LABEL_CLS}>name</label>
      <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="e.g. Summarize inbox" className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>description</label>
      <input value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} placeholder="One line on what it does" className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>type</label>
      <Select
        value={form.kind}
        options={[{ value: 'command', label: 'command (shell)' }, { value: 'prompt', label: 'prompt (model call)' }]}
        onChange={(v) => setForm({ ...form, kind: v as Script['kind'] })}
      />

      {form.kind === 'prompt' ? (
        <>
          <label className={`${FIELD_LABEL_CLS} mt-4`}>model</label>
          <Select
            value={form.model}
            options={models.map((m) => ({ value: m.id, label: m.label }))}
            onChange={(v) => setForm({ ...form, model: v })}
            placeholder="select model…"
          />
          <label className={`${FIELD_LABEL_CLS} mt-4`}>prompt</label>
          <textarea value={form.prompt} onChange={(e) => setForm({ ...form, prompt: e.target.value })} placeholder="What the model should do. Use {input} for the caller's text." rows={5} className={`${PROSE_CLS} min-h-[140px]`} />
        </>
      ) : (
        <>
          <label className={`${FIELD_LABEL_CLS} mt-4`}>command</label>
          <textarea value={form.command} onChange={(e) => setForm({ ...form, command: e.target.value })} placeholder="e.g. python build_report.py {input}" rows={4} className={`${PROSE_CLS} min-h-[120px]`} />
          <label className={`${FIELD_LABEL_CLS} mt-4`}>working directory (optional)</label>
          <input value={form.cwd} onChange={(e) => setForm({ ...form, cwd: e.target.value })} placeholder="e.g. A:\\projects\\reports" className={INPUT_CLS} />
        </>
      )}

      <div className="mt-4 flex items-center gap-2">
        <Button icon={<Play size={12} />} onClick={test}>test run</Button>
        {!form.id && <span className="font-mono text-[10px] text-muted">save first to test</span>}
      </div>
      {testOut !== null && (
        <pre className="mt-2 mb-0 max-h-40 overflow-auto rounded border border-border bg-background p-2 font-mono text-[11px] whitespace-pre-wrap text-foreground">{testOut || '(no output)'}</pre>
      )}

      {error && <p className="mt-2 mb-0 font-mono text-[10px] text-danger">{error}</p>}
    </Modal>
  );
}
