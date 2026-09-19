import { useState } from 'react';
import { Download, Loader2, X } from 'lucide-react';
import { installUpdate, restartApp, type UpdateInfo } from '../../runtime';

// In-app update prompt. Rust does the check/download/install; accepting here
// installs and restarts into the new version.
export function UpdatePrompt({ info, onDismiss }: { info: UpdateInfo; onDismiss: () => void }) {
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);

  const install = async () => {
    setInstalling(true);
    setError(undefined);
    try {
      await installUpdate();
      // The app relaunches into the installed version.
      await restartApp();
    } catch (e) {
      setInstalling(false);
      setError(typeof e === 'string' ? e : e instanceof Error ? e.message : 'Update failed.');
    }
  };

  return (
    <div className="fixed inset-0 z-[90] grid place-items-center bg-black/60">
      <div className="glass-strong w-full max-w-md animate-[dropdown-in_160ms_ease-out] rounded-2xl border border-line p-5 shadow-float">
        <div className="mb-2 flex items-center gap-2">
          <Download size={15} className="text-[var(--green)]" />
          <b className="text-[13px]">Update available — v{info.version}</b>
        </div>
        <p className="mb-3 text-[12px] text-muted">You're on v{info.currentVersion}.</p>
        {info.notes && (
          <pre className="mb-3 max-h-40 overflow-y-auto scrollbar-thin whitespace-pre-wrap rounded-lg border border-line bg-panel2 p-2.5 text-[11.5px] leading-1.6 text-text">{info.notes}</pre>
        )}
        {error && <p className="mb-2 text-[11px] text-[#f87171]">{error}</p>}
        <div className="flex justify-end gap-2">
          <button className="secondary" onClick={onDismiss} disabled={installing}><X size={13} />Later</button>
          <button className="primary" onClick={install} disabled={installing}>
            {installing
              ? <><Loader2 size={13} className="animate-spin" />Installing…</>
              : <><Download size={13} />Install &amp; restart</>}
          </button>
        </div>
      </div>
    </div>
  );
}
