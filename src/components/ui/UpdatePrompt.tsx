import { useState } from 'react';
import { Download, Loader2 } from 'lucide-react';
import { installUpdate, restartApp, type UpdateInfo } from '../../runtime';
import { Modal } from './Modal';
import { Button } from './Button';

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
    <Modal
      title={`update available — v${info.version}`}
      onClose={onDismiss}
      width="min(92vw, 460px)"
      height="auto"
      headerAction={
        <Button
          variant="primary"
          icon={installing ? <Loader2 size={13} className="animate-spin" /> : <Download size={13} />}
          disabled={installing}
          onClick={install}
        >
          {installing ? 'installing…' : 'install & restart'}
        </Button>
      }
    >
      <p className="m-0 font-mono text-xs text-muted">You're on v{info.currentVersion}.</p>
      {info.notes && (
        <pre className="mt-3 mb-0 max-h-40 overflow-x-hidden overflow-y-auto rounded border border-border bg-background p-2.5 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-foreground">{info.notes}</pre>
      )}
      {error && <p className="mt-2 mb-0 font-mono text-[11px] text-danger">{error}</p>}
    </Modal>
  );
}
