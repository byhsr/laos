import { useEffect } from 'react';
import { Loader2, Mic, Square } from 'lucide-react';
import { useDictation } from '../../hooks/useDictation';
import { toast } from '../../hooks/useToast';

// Click to start, click again to stop. The transcript is handed back to the
// composer for insertion — dictation never sends a message on its own.
export function MicButton({ onTranscript, disabled = false }: { onTranscript: (text: string) => void; disabled?: boolean }) {
  const modelId = useDictation((s) => s.modelId);
  const models = useDictation((s) => s.models);
  const recording = useDictation((s) => s.recording);
  const transcribing = useDictation((s) => s.transcribing);
  const loadModels = useDictation((s) => s.loadModels);

  useEffect(() => { void loadModels(); }, [loadModels]);

  const model = models.find((m) => m.id === modelId);

  const onClick = async () => {
    if (transcribing) return;
    const store = useDictation.getState();
    if (!recording) {
      // Only block when we know the catalog and the file is genuinely absent, so
      // a failed status load never stops a user who does have the model.
      if (model && !model.downloaded) {
        toast('Download the speech model in Settings → Dictation first.', 'error');
        return;
      }
      try { await store.start(); } catch (e) { toast(e instanceof Error ? e.message : 'Could not start recording.', 'error'); }
      return;
    }
    try {
      const text = await store.stop();
      if (text.trim()) onTranscript(text);
      else toast('No speech detected.', 'error');
    } catch (e) {
      toast(typeof e === 'string' ? e : 'Transcription failed.', 'error');
    }
  };

  const label = recording ? 'Stop dictation' : 'Dictate a message';

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled || transcribing}
      title={label}
      aria-label={label}
      className={`flex h-[38px] w-[38px] shrink-0 cursor-pointer items-center justify-center rounded-[12px] border transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
        recording
          ? 'animate-pulse border-[#f87171] bg-[#f87171]/15 text-[#f87171]'
          : 'border-line bg-panel2 text-muted hover:text-text'
      }`}
    >
      {transcribing ? <Loader2 size={14} className="animate-spin" /> : recording ? <Square size={13} /> : <Mic size={14} />}
    </button>
  );
}
