import { useState } from 'react';
import { createPortal } from 'react-dom';
import { Trash2 } from 'lucide-react';

// The app-wide destructive-action gate: the confirm button stays disabled until
// the entity's exact name is typed. Used for agents and workflows alike.
// Portaled to the root so an ancestor's transform/overflow can never displace or
// clip it.
export function DeleteConfirm({ name, title, description, confirmLabel = 'Delete', onCancel, onConfirm }: {
  name: string;
  title?: string;
  description?: string;
  confirmLabel?: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [typed, setTyped] = useState('');

  return createPortal(
    <div className="fixed inset-0 z-[90] grid place-items-center bg-black/60 p-6" onClick={onCancel}>
      <div
        className="w-[380px] max-w-[92vw] rounded-[16px] border border-line bg-panel p-5 shadow-[0_20px_60px_#000a] animate-[dropdown-in_160ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-1 flex items-center gap-2">
          <Trash2 size={15} className="text-[#f87171]" />
          <b className="text-[14px]">{title ?? `Delete "${name}"?`}</b>
        </div>
        <p className="mb-4 text-[12px] leading-1.6 text-muted">
          {description ?? `This permanently removes it. Type the name to confirm.`}
        </p>
        <input
          autoFocus
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          placeholder={name}
          className="mb-4 w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[13px] text-text outline-none focus:border-mid"
        />
        <div className="flex justify-end gap-2">
          <button className="secondary" onClick={onCancel}>Cancel</button>
          <button
            className="primary disabled:cursor-not-allowed disabled:opacity-40"
            style={{ background: '#e11d48', color: '#fff' }}
            disabled={typed.trim() !== name}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
