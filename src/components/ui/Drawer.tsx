import { X } from 'lucide-react';

export function Drawer({ title, onClose, children }: { title: string; onClose: () => void; children: React.ReactNode }) {
  return (
    <>
      <div className="fixed inset-0 z-20 bg-black/50" onClick={onClose} />
      <div className="fixed top-0 right-0 bottom-0 z-[21] flex w-[min(520px,92vw)] animate-[drawer-in_180ms_ease-out] flex-col border-l border-line bg-panel shadow-[-18px_0_40px_#0008]" role="dialog" aria-label={title}>
        <div className="flex h-[60px] flex-none items-center justify-between border-b border-line px-[22px]">
          <b style={{ fontSize: 15 }}>{title}</b>
          <button className="secondary" onClick={onClose}><X size={13} />Close</button>
        </div>
        <div className="flex-1 overflow-y-auto p-[22px]">{children}</div>
      </div>
    </>
  );
}
