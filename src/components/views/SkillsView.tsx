import { useState } from 'react';
import { Check, Plus, Sparkles, Trash2 } from 'lucide-react';
import type { Skill } from '../../types';
import { Drawer } from '../ui/Drawer';

export function SkillsView({ skills, onAdd, onEdit, onDelete, embedded = false }: {
  skills: Skill[]; onAdd: () => void; onEdit: (s: Skill) => void; onDelete: (id: string) => Promise<void>; embedded?: boolean;
}) {
  const wordCount = (s: Skill) => (s.content.trim() ? `${s.content.trim().split(/\s+/).length} words` : 'empty');

  return (
    <>
      <header className="mb-6 flex items-end justify-between gap-4">
        {!embedded && (
          <div className="min-w-0">
            <span className="font-mono text-[11px] tracking-[1px] text-muted">WORKSPACE</span>
            <h1 className="m-0 text-[24px]">Skills</h1>
            <p className="mt-1 text-[12px] leading-[1.6] text-muted">Reusable instruction packs. Attach them to an agent and they are injected into its prompt on every run and chat turn.</p>
          </div>
        )}
        <button className="primary ml-auto shrink-0" onClick={onAdd}><Check size={13} />Add skill</button>
      </header>

      <div className="grid max-w-[900px] gap-3">
        {skills.map((s) => (
          <div key={s.id} className="flex items-center gap-[15px] rounded-[16px] border border-line bg-panel p-[22px]">
            <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-panel2 text-muted"><Sparkles size={18} /></span>
            <div className="min-w-0 flex-1">
              <b className="text-[13px]">{s.name}</b>
              <span className="block truncate text-[11px] text-muted">{s.description || 'No description'}</span>
              <span className="mt-1 block font-mono text-[11px] text-muted">{wordCount(s)}</span>
            </div>
            <div className="flex shrink-0 gap-1.5">
              <button className="secondary" onClick={() => onEdit(s)}>Edit</button>
              <button className="secondary" title="Delete skill" onClick={() => onDelete(s.id)}><Trash2 size={12} /></button>
            </div>
          </div>
        ))}
        {skills.length === 0 && (
          <p className="mt-[22px] text-[12px] text-muted">No skills yet. Add one, then attach it to an agent in the agent's Config tab.</p>
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

  const inputCls = 'w-full rounded-[10px] border border-line bg-panel2 px-3 py-2.5 text-text';
  const labelCls = 'mt-4 mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase';

  return (
    <Drawer
      title={isNew ? 'Add skill' : `Edit ${form.name}`}
      onClose={onClose}
      initialWidth={Math.round(window.innerWidth / 2)}
      resizable
      headerAction={<button className="primary" disabled={saving} onClick={save}><Check size={13} />{saving ? 'Saving…' : 'Save'}</button>}
    >
      <div className="flex h-[calc(100vh-170px)] min-h-[420px] flex-col rounded-[16px] border border-line bg-panel p-[22px]">
        <p className="m-0 mb-3.5 text-[12px] leading-[1.6] text-muted">
          A skill is instructions the model follows. Every agent you attach it to gets this text in its system prompt.
        </p>

        <label className={labelCls}>NAME</label>
        <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="e.g. Cold email writer" className={inputCls} />

        <label className={labelCls}>WHEN TO USE</label>
        <input value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} placeholder="One line describing when this applies" className={inputCls} />

        <label className={labelCls}>INSTRUCTIONS</label>
        <textarea
          value={form.content}
          onChange={(e) => setForm({ ...form, content: e.target.value })}
          placeholder="Write the instructions the agent should follow…"
          className={`${inputCls} min-h-0 flex-1 resize-none leading-[1.6]`}
        />

        {error && <p className="m-0 mt-2 text-[11px] text-[#f87171]">{error}</p>}
      </div>
    </Drawer>
  );
}
