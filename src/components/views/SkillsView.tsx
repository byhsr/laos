import { useState, type DragEvent } from 'react';
import { Check, Download, Sparkles, Trash2, Upload } from 'lucide-react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import type { Script, Skill } from '../../types';
import { exportSkills, importSkillFiles, importSkills, type SkillDraft, type SkillImportReport } from '../../runtime';
import { toast } from '../../hooks/useToast';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { MultiDropdown } from '../ui/MultiDropdown';
import { FIELD_LABEL_CLS, INPUT_CLS, PROSE_CLS } from '../ui/Input';

export function SkillsView({ skills, onAdd, onEdit, onDelete, onImport, onExport }: {
  skills: Skill[]; onAdd: () => void; onEdit: (s: Skill) => void; onDelete: (id: string) => Promise<void>;
  onImport: () => void; onExport: () => void;
}) {
  const summary = (s: Skill) => {
    const words = (s.content ?? '').trim() ? `${s.content.trim().split(/\s+/).length} words` : 'no instructions';
    const bits = [words];
    if ((s.docs ?? '').trim()) bits.push('reference');
    if ((s.scriptIds ?? []).length) bits.push(`${s.scriptIds.length} script${s.scriptIds.length === 1 ? '' : 's'}`);
    return bits.join(' · ');
  };

  return (
    <>
      <div className="mb-4 flex justify-end gap-1.5">
        <Button icon={<Upload size={13} />} onClick={onImport}>import</Button>
        <Button icon={<Download size={13} />} onClick={onExport}>export</Button>
        <Button variant="primary" icon={<Check size={13} />} onClick={onAdd}>add skill</Button>
      </div>

      <div className="grid gap-2">
        {skills.map((s) => (
          <Card key={s.id} className="flex items-center gap-4 hover:bg-surface">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-muted"><Sparkles size={16} /></span>
            <div className="min-w-0 flex-1">
              <b className="block truncate font-mono text-[11px] text-foreground">{s.name}</b>
              <span className="block truncate font-mono text-[10px] text-muted">{s.description || 'no description'}</span>
              <span className="mt-0.5 block font-mono text-[10px] text-muted">{summary(s)}</span>
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

export function SkillFormDrawer({ editing, isNew, scripts, onClose, onSave }: {
  editing: Skill; isNew: boolean; scripts: Script[]; onClose: () => void; onSave: (s: Skill) => Promise<void>;
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
          A skill is instructions the model follows, plus optional reference material and scripts it can run.
          Every agent you attach it to gets the instructions and reference in its prompt.
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
          className={`${PROSE_CLS} min-h-[200px] flex-1`}
        />

        <label className={`${FIELD_LABEL_CLS} mt-4`}>reference (optional)</label>
        <textarea
          value={form.docs}
          onChange={(e) => setForm({ ...form, docs: e.target.value })}
          placeholder="Reference material the agent can consult: formats, examples, checklists…"
          className={`${PROSE_CLS} min-h-[120px]`}
        />

        <label className={`${FIELD_LABEL_CLS} mt-4`}>scripts (optional)</label>
        <MultiDropdown
          values={form.scriptIds}
          options={scripts.map((s) => ({ value: s.id, label: s.name }))}
          onChange={(v) => setForm({ ...form, scriptIds: v })}
          placeholder="none attached"
        />
        <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">Saved scripts the agent can run with the run_script tool (needs host file access).</p>

        {error && <p className="mt-2 mb-0 font-mono text-[10px] text-danger">{error}</p>}
      </div>
    </Modal>
  );
}

// Reads every .md under the dropped entries (folders recurse). The webview has
// the file contents but not a usable path, so the backend is handed the markdown
// itself — the same approach `ChatComposer` uses for dropped attachments.
async function readDropped(entries: FileSystemEntry[]): Promise<SkillDraft[]> {
  const out: SkillDraft[] = [];
  const walk = async (entry: FileSystemEntry) => {
    if (entry.isFile) {
      const file = await new Promise<File>((resolve, reject) => (entry as FileSystemFileEntry).file(resolve, reject));
      if (/\.md$/i.test(file.name)) out.push({ name: file.name, content: await file.text() });
    } else if (entry.isDirectory) {
      const reader = (entry as FileSystemDirectoryEntry).createReader();
      const next = () => new Promise<FileSystemEntry[]>((resolve) => reader.readEntries(resolve, () => resolve([])));
      // readEntries returns at most 100 at a time — keep going until it is empty.
      for (let batch = await next(); batch.length; batch = await next()) {
        for (const child of batch) await walk(child);
      }
    }
  };
  for (const entry of entries) await walk(entry);
  return out;
}

// Import / export skills as `SKILL.md` files (the Agent Skills format). Import
// takes a folder via the native picker or markdown dropped onto the panel;
// export writes one `SKILL.md` per skill.
export function SkillsTransferModal({ mode, onClose, onImported }: {
  mode: 'import' | 'export';
  onClose: () => void;
  onImported: () => Promise<void> | void;
}) {
  const isImport = mode === 'import';
  const [path, setPath] = useState('');
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const [result, setResult] = useState<string | undefined>(undefined);

  const summarize = (r: SkillImportReport) => {
    const bits = [`${r.imported} imported`, `${r.updated} updated`];
    if (r.skipped) bits.push(`${r.skipped} skipped`);
    return bits.join(' · ');
  };

  const withBusy = async (work: () => Promise<string>) => {
    setBusy(true); setError(undefined); setResult(undefined);
    try {
      setResult(await work());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const run = () => withBusy(async () => {
    if (isImport) {
      if (!path.trim()) throw new Error('Choose a folder (or type a path) first.');
      const r = await importSkills(path.trim());
      await onImported();
      toast(`skills: ${summarize(r)}`, 'success');
      return summarize(r);
    }
    const r = await exportSkills(path.trim());
    toast('skills exported', 'success');
    return `exported ${r.count} skill${r.count === 1 ? '' : 's'} to ${r.dir}`;
  });

  const importDropped = (drafts: SkillDraft[]) => withBusy(async () => {
    if (!drafts.length) throw new Error('No .md files in what you dropped.');
    const r = await importSkillFiles(drafts);
    await onImported();
    toast(`skills: ${summarize(r)}`, 'success');
    return summarize(r);
  });

  const browse = async () => {
    try {
      const picked = await openDialog({
        directory: true, multiple: false,
        title: isImport ? 'Choose a skills folder' : 'Choose an export folder',
      });
      if (typeof picked === 'string') setPath(picked);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setDragging(false);
    // dataTransfer entries are only valid during the event — read them now.
    const entries: FileSystemEntry[] = [];
    for (const item of Array.from(e.dataTransfer.items ?? [])) {
      const entry = (item as DataTransferItem & { webkitGetAsEntry?: () => FileSystemEntry | null }).webkitGetAsEntry?.();
      if (entry) entries.push(entry);
    }
    if (entries.length) { void readDropped(entries).then(importDropped); return; }
    const files = Array.from(e.dataTransfer.files ?? []).filter((f) => /\.md$/i.test(f.name));
    void Promise.all(files.map(async (f) => ({ name: f.name, content: await f.text() }))).then(importDropped);
  };

  return (
    <Modal
      title={isImport ? 'import skills' : 'export skills'}
      onClose={onClose}
      headerAction={<Button variant="primary" disabled={busy} onClick={run}>{busy ? (isImport ? 'importing…' : 'exporting…') : isImport ? 'import' : 'export'}</Button>}
    >
      <div className="flex min-h-full flex-col">
        <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
          {isImport
            ? 'Import skills in the Agent Skills format: each file is markdown with a name and description in its frontmatter. A skill whose name already exists is updated in place.'
            : 'Each skill is written as its own SKILL.md file. Leave the folder empty to export to Downloads.'}
        </p>

        {isImport && (
          <div
            onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
            onDragLeave={() => setDragging(false)}
            onDrop={onDrop}
            className={`mb-3 grid h-[88px] place-items-center rounded-lg border border-dashed text-center font-mono text-[10px] text-muted transition-colors ${dragging ? 'border-foreground/60 bg-background' : 'border-border'}`}
          >
            drop a folder or .md files here
          </div>
        )}

        <label className={FIELD_LABEL_CLS}>{isImport ? 'or choose a folder' : 'destination folder (optional)'}</label>
        <div className="flex items-center gap-1.5">
          <input
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder={isImport ? 'A:\\code\\skills' : 'defaults to Downloads\\lup-skills'}
            className={`${INPUT_CLS} min-w-0 flex-1`}
          />
          <Button className="shrink-0" onClick={browse}>browse</Button>
        </div>

        {result && <p className="mt-3 mb-0 font-mono text-[10px] leading-relaxed text-foreground">{result}</p>}
        {error && <p className="mt-2 mb-0 font-mono text-[10px] leading-relaxed text-danger">{error}</p>}
      </div>
    </Modal>
  );
}
