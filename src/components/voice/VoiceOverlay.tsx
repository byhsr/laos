import { useEffect, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { Mic, Settings2, Square, Volume2, X } from 'lucide-react';
import { Button } from '../ui/Button';
import { useVoice, type VoicePhase } from '../../hooks/useVoice';
import { useManagerStore, type ChatEntry } from '../../hooks/useManager';
import { useAgentsStore } from '../../hooks/useAgents';

const EMPTY: ChatEntry[] = [];

const PHASE_LABEL: Record<Exclude<VoicePhase, 'idle'>, string> = {
  listening: 'listening…',
  transcribing: 'transcribing…',
  thinking: 'thinking…',
  speaking: 'speaking…',
  error: 'voice mode stopped',
};

// The hands-free voice surface. Shown only while a session is active; the hotkey,
// Esc, the close button or clicking outside all end it.
export function VoiceOverlay({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const active = useVoice((s) => s.active);
  const phase = useVoice((s) => s.phase);
  const level = useVoice((s) => s.level);
  const transcript = useVoice((s) => s.transcript);
  const error = useVoice((s) => s.error);
  const agentId = useVoice((s) => s.agentId);
  const stop = useVoice((s) => s.stop);

  const agentName = useAgentsStore((s) => s.agents.find((a) => a.id === agentId)?.name ?? 'assistant');
  const conversation = useManagerStore((s) => (agentId ? s.conversations[agentId] : undefined)) ?? EMPTY;
  const reply = useMemo(
    () => [...conversation].reverse().find((m) => m.role === 'assistant')?.content ?? '',
    [conversation],
  );

  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); stop(); } };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [active, stop]);

  if (!active) return null;
  const live = phase === 'idle' ? 'listening…' : PHASE_LABEL[phase];

  return createPortal(
    <div className="fixed inset-0 z-[120] grid place-items-center bg-black/60 p-6 backdrop-blur-sm" onMouseDown={stop}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="live voice"
        className="w-[min(540px,94vw)] rounded-xl border border-border bg-background/95 p-5 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="flex items-center justify-between gap-3">
          <span className="font-mono text-xs lowercase text-foreground">live voice</span>
          <button
            type="button"
            aria-label="exit voice mode"
            className="cursor-pointer border-0 bg-transparent p-1 text-muted hover:text-foreground"
            onClick={stop}
          >
            <X size={14} />
          </button>
        </header>

        <div className="mt-4 flex flex-col items-center gap-3">
          {/* A capture ring that breathes with the microphone level. */}
          <div className="relative grid h-28 w-28 place-items-center">
            <div
              aria-hidden
              className="absolute inset-0 rounded-full border border-border/70 transition-transform duration-75"
              style={{ transform: `scale(${1 + Math.min(level, 1) * 0.22})` }}
            />
            <div
              className={`grid h-20 w-20 place-items-center rounded-full border bg-surface transition-colors ${
                phase === 'speaking' ? 'border-foreground/50 text-foreground' : 'border-border text-muted'
              }`}
            >
              {phase === 'speaking' ? <Volume2 size={22} /> : <Mic size={22} />}
            </div>
          </div>

          {/* Level bars, so silence vs speech is visible. */}
          <div aria-hidden className="flex h-8 items-center gap-[3px]">
            {Array.from({ length: 28 }, (_, i) => {
              const h = phase === 'listening' ? 4 + Math.min(level, 1) * 120 * (0.3 + Math.abs(Math.sin(i * 0.9)) * 0.7) : 4;
              return <span key={i} className="w-[3px] rounded-full bg-foreground/70" style={{ height: `${Math.min(32, h)}px` }} />;
            })}
          </div>

          <p className="m-0 font-mono text-[11px] lowercase text-muted">
            {live} <span className="text-muted/60">· {agentName}</span>
          </p>
        </div>

        {transcript && (
          <div className="mt-4 rounded-lg border border-border bg-surface px-3 py-2">
            <span className="block font-mono text-[9px] lowercase text-muted">you said</span>
            <p className="mt-0.5 mb-0 text-[12px] leading-relaxed text-foreground">{transcript}</p>
          </div>
        )}

        {(phase === 'thinking' || phase === 'speaking') && reply && (
          <div className="mt-2 rounded-lg border border-border bg-surface px-3 py-2">
            <span className="block font-mono text-[9px] lowercase text-muted">{agentName}</span>
            <p className="mt-0.5 mb-0 line-clamp-4 text-[12px] leading-relaxed text-foreground">{reply}</p>
          </div>
        )}

        {phase === 'error' && (
          <div className="mt-4 rounded-lg border border-danger/40 bg-danger/5 px-3 py-2">
            <p className="m-0 font-mono text-[11px] leading-relaxed text-danger">{error ?? 'voice mode could not start'}</p>
            {onOpenSettings && (
              <div className="mt-2">
                <Button icon={<Settings2 size={12} />} onClick={() => { onOpenSettings(); stop(); }}>
                  open settings
                </Button>
              </div>
            )}
          </div>
        )}

        <div className="mt-5 flex items-center justify-center gap-2">
          <Button icon={<Square size={12} />} onClick={stop}>stop</Button>
          <span className="font-mono text-[10px] text-muted">esc · or the hotkey</span>
        </div>
      </div>
    </div>,
    document.body,
  );
}
