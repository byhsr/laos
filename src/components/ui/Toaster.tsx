import { CheckCircle2, Info, X, XCircle } from 'lucide-react';
import { useToastStore, type ToastKind } from '../../hooks/useToast';

// One toast shell. Only the error variant carries hue — every other kind is
// neutral chrome.
const KIND_STYLES: Record<ToastKind, string> = {
  success: 'border-border bg-background text-foreground',
  error: 'border-danger bg-danger text-white',
  info: 'border-border bg-background text-foreground',
};

const KIND_ICON: Record<ToastKind, React.ReactNode> = {
  success: <CheckCircle2 size={13} />,
  error: <XCircle size={13} />,
  info: <Info size={13} />,
};

export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);

  return (
    <div className="pointer-events-none fixed top-14 right-4 z-[9999] flex flex-col gap-2" style={{ width: 'clamp(240px, 20vw, 320px)' }}>
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`pointer-events-auto flex animate-[ip-toast_250ms_ease-out_both] items-start gap-2.5 rounded-lg border px-3 py-2 font-mono text-xs ${KIND_STYLES[t.kind]}`}
        >
          <span className="mt-px shrink-0">{KIND_ICON[t.kind]}</span>
          <span className="min-w-0 flex-1 leading-relaxed break-words whitespace-pre-wrap">{t.message}</span>
          <button
            type="button"
            aria-label="dismiss"
            className="shrink-0 cursor-pointer border-0 bg-transparent p-0 opacity-60 transition-opacity hover:opacity-100"
            onClick={() => dismiss(t.id)}
          >
            <X size={12} />
          </button>
        </div>
      ))}
    </div>
  );
}
