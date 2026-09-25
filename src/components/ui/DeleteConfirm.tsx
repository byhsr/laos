import { useState } from 'react';
import { Trash2 } from 'lucide-react';
import { Modal } from './Modal';
import { Button } from './Button';
import { INPUT_CLS } from './Input';

// The app-wide destructive-action gate: the confirm button stays disabled until
// the entity's exact name is typed. Same gate for every destructible entity.
export function DeleteConfirm({ name, title, description, confirmLabel = 'delete', onCancel, onConfirm }: {
  name: string;
  title?: string;
  description?: string;
  confirmLabel?: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [typed, setTyped] = useState('');

  return (
    <Modal
      title={title ?? `delete "${name}"?`}
      onClose={onCancel}
      width="min(92vw, 400px)"
      height="auto"
    >
      <div className="mb-2 flex items-center gap-2">
        <Trash2 size={14} className="shrink-0 text-danger" />
        <b className="font-mono text-xs text-foreground">
          {description ?? 'This permanently removes it. Type the name to confirm.'}
        </b>
      </div>
      <input
        autoFocus
        value={typed}
        onChange={(e) => setTyped(e.target.value)}
        placeholder={name}
        className={`${INPUT_CLS} mb-4 mt-3`}
      />
      <div className="flex justify-end gap-2">
        <Button onClick={onCancel}>cancel</Button>
        <Button variant="danger" disabled={typed.trim() !== name} onClick={onConfirm}>{confirmLabel}</Button>
      </div>
    </Modal>
  );
}
