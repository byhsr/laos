import { CheckCircle2, Info, X, XCircle } from 'lucide-react';
import { useToastStore, type ToastKind } from '../../hooks/useToast';

const KIND_STYLES: Record<ToastKind, string> = {
  success: 'border-[var(--green)]/50 text-[var(--green)]',
  error: 'border-[#f87171]/50 text-[#f87171]',
  info: 'border-mid/50 text-muted',
};

const KIND_ICON: Record<ToastKind, React.ReactNode> = {
  success: <CheckCircle2 size={15} />,
  error: <XCircle size={15} />,
  info: <Info size={15} />,
};

export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);

  return (
    <div className="pointer-events-none fixed top-10 right-4 z-[100] flex w-80 flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`pointer-events-auto flex animate-[dropdown-in_140ms_ease-out] items-center gap-2.5 rounded-lg border border-line bg-panel px-3.5 py-2.5 shadow-[0_10px_30px_#000a] ${KIND_STYLES[t.kind]}`}
        >
          {KIND_ICON[t.kind]}
          <span className="flex-1 text-[12px] leading-1.5 text-text">{t.message}</span>
          <button className="cursor-pointer border-0 bg-transparent p-0 text-muted hover:text-text" onClick={() => dismiss(t.id)}><X size={12} /></button>
        </div>
      ))}
    </div>
  );
}
