import { useState } from 'react';
import { Check, FolderKanban, Trash2 } from 'lucide-react';
import type { Project } from '../../types';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, INPUT_CLS, PROSE_CLS } from '../ui/Input';

export function ProjectsView({ projects, activeId, onAdd, onEdit, onDelete, onSetActive }: {
  projects: Project[]; activeId: string; onAdd: () => void; onEdit: (p: Project) => void; onDelete: (id: string) => Promise<void>; onSetActive: (id: string) => void;
}) {
  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button variant="primary" icon={<Check size={13} />} onClick={onAdd}>add project</Button>
      </div>

      <p className="mt-0 mb-3 font-mono text-[10px] leading-relaxed text-muted">
        A project groups conversations and scopes their memory. Set one active and new chats are filed under it, so the same project can be tracked across conversations.
      </p>

      <div className="grid gap-2">
        {projects.map((p) => (
          <Card key={p.id} className="flex items-center gap-4 hover:bg-surface">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-muted"><FolderKanban size={16} /></span>
            <div className="min-w-0 flex-1">
              <b className="block truncate font-mono text-[11px] text-foreground">{p.name}{p.id === activeId ? ' · active' : ''}</b>
              <span className="block truncate font-mono text-[10px] text-muted">{p.description || 'no description'}</span>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              <Button variant={p.id === activeId ? 'primary' : 'default'} onClick={() => onSetActive(p.id === activeId ? '' : p.id)}>{p.id === activeId ? 'active' : 'set active'}</Button>
              <Button onClick={() => onEdit(p)}>edit</Button>
              <IconButton label="delete project" onClick={() => void onDelete(p.id)}><Trash2 size={12} /></IconButton>
            </div>
          </Card>
        ))}
        {projects.length === 0 && <p className="m-0 font-mono text-[11px] text-muted">no projects yet — add one to group and track work</p>}
      </div>
    </>
  );
}

export function ProjectFormDrawer({ editing, isNew, onClose, onSave }: {
  editing: Project; isNew: boolean; onClose: () => void; onSave: (p: Project) => Promise<void>;
}) {
  const [form, setForm] = useState<Project>(editing);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);

  const save = async () => {
    if (!form.name.trim()) { setError('Project needs a name.'); return; }
    setSaving(true); setError(undefined);
    try {
      await onSave({ ...form, name: form.name.trim(), description: form.description.trim() });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save project.');
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={isNew ? 'add project' : `edit ${form.name}`}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={saving} onClick={save}>{saving ? 'saving…' : 'save'}</Button>}
    >
      <label className={FIELD_LABEL_CLS}>name</label>
      <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="e.g. Q4 launch" className={INPUT_CLS} />

      <label className={`${FIELD_LABEL_CLS} mt-4`}>description</label>
      <textarea value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} placeholder="What this project is about" rows={3} className={PROSE_CLS} />

      {error && <p className="mt-2 mb-0 font-mono text-[10px] text-danger">{error}</p>}
    </Modal>
  );
}
