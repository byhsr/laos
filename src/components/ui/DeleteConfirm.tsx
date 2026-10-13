import { Trash2 } from 'lucide-react';
import { Modal } from './Modal';
import { Button } from './Button';

// The app-wide destructive-action gate: one simple confirm popup. Every
// destructible entity uses this same gate.
export function DeleteConfirm({ name, title, description, confirmLabel = 'delete', onCancel, onConfirm }: {
  name: string;
  title?: string;
  description?: string;
  confirmLabel?: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Modal
      title={title ?? `delete "${name}"?`}
      onClose={onCancel}
      width="min(92vw, 400px)"
      height="auto"
    >
      <div className="mb-4 flex items-center gap-2">
        <Trash2 size={14} className="shrink-0 text-danger" />
        <b className="font-mono text-xs text-foreground">
          {description ?? 'This permanently removes it — there is no undo.'}
        </b>
      </div>
      <div className="flex justify-end gap-2">
        <Button onClick={onCancel}>cancel</Button>
        <Button variant="danger" autoFocus onClick={onConfirm}>{confirmLabel}</Button>
      </div>
    </Modal>
  );
}
