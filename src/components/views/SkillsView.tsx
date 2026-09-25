import { useState } from 'react';
import { Check, Sparkles, Trash2 } from 'lucide-react';
import type { Skill } from '../../types';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, INPUT_CLS, PROSE_CLS } from '../ui/Input';

export function SkillsView({ skills, onAdd, onEdit, onDelete }: {
  skills: Skill[]; onAdd: () => void; onEdit: (s: Skill) => void; onDelete: (id: string) => Promise<void>;
}) {
  const wordCount = (s: Skill) => (s.content.trim() ? `${s.content.trim().split(/\s+/).length} words` : 'empty');

  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button variant="primary" icon={<Check size={13} />} onClick={onAdd}>add skill</Button>
      </div>

      <div className="grid gap-2">
        {skills.map((s) => (
          <Card key={s.id} className="flex items-center gap-4 hover:bg-surface">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-muted"><Sparkles size={16} /></span>
            <div className="min-w-0 flex-1">
              <b className="block truncate font-mono text-[11px] text-foreground">{s.name}</b>
              <span className="block truncate font-mono text-[10px] text-muted">{s.description || 'no description'}</span>
              <span className="mt-0.5 block font-mono text-[10px] text-muted">{wordCount(s)}</span>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              <Button onClick={() => onEdit(s)}>edit</Button>
              <IconButton label="delete skill" onClick={() => onDelete(s.id)}><Trash2 size={12} /></IconButton>
            </div>
          </Card>
        ))}
        {skills.length === 0 && (
          <p className="m-0 font-mono text-[11px] text-muted">no skills yet — add one, then attach it to an agent in the agent's config tab</p>
        )}
      </div>
    </>
  );
}

export function SkillFormDrawer({ editing, isNew, onClose, onSave }: {
  editing: Skill; isNew: boolean; onClose: () => void; onSave: (s: Skill) => Promise<void>;
}) {
  const [form, setForm] = useState<Skill>(editing);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);

  const save = async () => {
    if (!form.name.trim()) { setError('Skill needs a name.'); return; }
    if (!form.content.trim()) { setError('Skill needs instructions.'); return; }
    setSaving(true); setError(undefined);
    try {
      await onSave({ ...form, id: form.id || `skill-${Date.now()}`, name: form.name.trim(), description: form.description.trim() });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save skill.');
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={isNew ? 'add skill' : `edit ${form.name}`}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
    >
      <div className="flex min-h-full flex-col">
        <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
          A skill is instructions the model follows. Every agent you attach it to gets this text in its system prompt.
        </p>

        <label className={FIELD_LABEL_CLS}>name</label>
        <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="e.g. Cold email writer" className={INPUT_CLS} />

        <label className={`${FIELD_LABEL_CLS} mt-4`}>when to use</label>
        <input value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} placeholder="One line describing when this applies" className={INPUT_CLS} />

        <label className={`${FIELD_LABEL_CLS} mt-4`}>instructions</label>
        <textarea
          value={form.content}
          onChange={(e) => setForm({ ...form, content: e.target.value })}
          placeholder="Write the instructions the agent should follow…"
          className={`${PROSE_CLS} min-h-[240px] flex-1`}
        />

        {error && <p className="mt-2 mb-0 font-mono text-[10px] text-danger">{error}</p>}
      </div>
    </Modal>
  );
}
