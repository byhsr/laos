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
    <div className="pointer-events-none fixed top-10 right-4 z-[100] flex min-w-80 max-w-md flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`glass-strong pointer-events-auto flex animate-[dropdown-in_160ms_ease-out] items-start gap-3 rounded-2xl border border-hairline px-4 py-3 shadow-float ${KIND_STYLES[t.kind]}`}
        >
          {KIND_ICON[t.kind]}
          <span className="min-h-4 flex-1 whitespace-pre-wrap break-words text-[12px] leading-relaxed text-text">{t.message}</span>
          <button className="cursor-pointer border-0 bg-transparent p-0 text-muted hover:text-text" onClick={() => dismiss(t.id)}><X size={12} /></button>
        </div>
      ))}
    </div>
  );
}
