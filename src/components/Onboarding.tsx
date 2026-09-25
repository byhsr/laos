import { useEffect, useState } from 'react';
import { ArrowLeft, ArrowRight, Check } from 'lucide-react';
import { Lottie } from 'lottie-react';
import type { Agent, ModelConfig } from '../types';
import { PERSONAS } from './ui/AgentAvatar';
import { Button } from './ui/Button';
import { Tooltip } from './ui/Tooltip';
import { FIELD_LABEL_CLS, INPUT_CLS, PROSE_CLS } from './ui/Input';

export type OnboardingPatch = { name: string; objective: string; persona: string; model: string };

const STEPS = ['welcome', 'name', 'purpose', 'model'] as const;

const personaData = (id: string) => PERSONAS.find((p) => p.id === id)?.data ?? PERSONAS[0].data;

// First-run setup — a full-screen brand moment, so the animated persona stage is
// allowed to be the one expressive element. Its chrome still follows the shell:
// surface planes, hairline borders, mono labels, no glow.
export function Onboarding({ manager, models, onFinish, onSkip }: {
  manager: Agent; models: ModelConfig[];
  onFinish: (patch: OnboardingPatch) => Promise<void>;
  onSkip: () => void;
}) {
  const [step, setStep] = useState(0);
  const [name, setName] = useState(manager.name);
  const [persona, setPersona] = useState(manager.persona ?? 'ai-orb');
  const [objective, setObjective] = useState(manager.objective);
  const [model, setModel] = useState(manager.model || models.find((m) => m.enabled)?.id || '');
  const [hovered, setHovered] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const enabledModels = models.filter((m) => m.enabled);
  const agentName = name.trim() || 'your agent';
  const pickedModel = enabledModels.find((m) => m.id === model);
  const last = step === STEPS.length - 1;

  // Enter advances, except while writing the prompt.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const inTextarea = e.target instanceof HTMLTextAreaElement;
      if (e.key === 'Enter' && !e.shiftKey && !inTextarea && step > 0 && step < STEPS.length - 1) setStep((s) => s + 1);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [step]);

  const finish = async () => {
    setSaving(true);
    try {
      await onFinish({ name: name.trim() || manager.name, objective: objective.trim(), persona, model });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-40 flex flex-col overflow-hidden bg-background">
      {/* Progress — completed steps are clickable to go back */}
      <header className="relative z-10 flex shrink-0 flex-wrap items-center justify-center gap-1.5 px-6 pt-6">
        {STEPS.map((s, i) => (
          <button
            key={s}
            disabled={i > step}
            onClick={() => setStep(i)}
            className={`focus-ring flex items-center gap-2 rounded-lg border px-2.5 py-1 font-mono text-[11px] lowercase transition-colors duration-150 disabled:cursor-not-allowed ${
              i === step
                ? 'border-border bg-surface text-foreground'
                : i < step
                  ? 'border-transparent text-muted hover:bg-surface hover:text-foreground'
                  : 'border-transparent text-muted/50'
            }`}
          >
            <span className={`grid h-4 w-4 place-items-center rounded-[4px] font-mono text-[9px] ${i <= step ? 'bg-foreground text-background' : 'bg-border text-muted'}`}>{i + 1}</span>
            {s}
          </button>
        ))}
      </header>

      <div className="relative z-10 flex min-h-0 flex-1 items-center justify-center overflow-x-hidden overflow-y-auto px-6 py-6">
        {step === 0 ? (
          <div className="flex flex-col items-center text-center animate-[ip-fade_250ms_var(--ease-panel)_both]">
            <div className="mb-2 grid h-[220px] w-[220px] place-items-center">
              <Lottie src={personaData(persona)} loop autoplay style={{ width: 200, height: 200 }} />
            </div>
            <h1 className="m-0 max-w-[620px] text-[30px] leading-[1.15] font-bold">Your workspace, run by an agent you shape</h1>
            <p className="mt-3 max-w-[540px] text-[13px] leading-[1.7] text-muted">
              Everything runs on this machine — agents, tools, chats and runs stay local. One lead agent
              orchestrates the rest, and it's yours to name and direct.
            </p>
            <Button variant="accent" className="mt-7" icon={<ArrowRight size={13} />} onClick={() => setStep(1)}>
              get started
            </Button>
          </div>
        ) : (
          <div key={step} className="grid w-full max-w-[980px] grid-cols-1 items-start gap-10 animate-[ip-fade_200ms_var(--ease-panel)_both] lg:grid-cols-[300px_minmax(0,1fr)]">
            {/* Live stage — reflects the choices as they're made */}
            <div className="flex flex-col items-center">
              <div className="grid h-[190px] w-[190px] place-items-center">
                <Lottie src={personaData(persona)} loop autoplay style={{ width: 170, height: 170 }} />
              </div>
              <b className="mt-1 max-w-[260px] truncate text-[15px]">{name.trim() || 'Your lead agent'}</b>
              <span className="mt-1 max-w-[260px] truncate font-mono text-[10px] text-muted">{pickedModel?.label ?? 'no model selected'}</span>
            </div>

            <div className="min-w-0">
              {step === 1 && (
                <div>
                  <h2 className="m-0 text-[20px] font-bold">Name your lead agent</h2>
                  <p className="mt-1.5 text-[13px] leading-[1.65] text-muted">Call it whatever you like — this is who you'll be talking to.</p>
                  <input
                    autoFocus
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="Chief, Ada, Boss…"
                    className={`${INPUT_CLS} mt-5 max-w-[440px] py-2.5`}
                  />
                  <label className={`${FIELD_LABEL_CLS} mt-6`}>look</label>
                  <div className="grid max-w-[440px] grid-cols-5 gap-1.5 sm:grid-cols-6">
                    {PERSONAS.map((p) => {
                      const active = p.id === persona;
                      return (
                        <Tooltip key={p.id} label={p.label} className="flex w-full">
                          <button
                            type="button"
                            onClick={() => setPersona(p.id)}
                            onMouseEnter={() => setHovered(p.id)}
                            onMouseLeave={() => setHovered((h) => (h === p.id ? null : h))}
                            className={`focus-ring grid w-full cursor-pointer place-items-center rounded-lg border p-1.5 transition-colors duration-150 ${
                              active ? 'border-foreground/40 bg-surface' : 'border-border hover:bg-surface'
                            }`}
                          >
                            <Lottie src={p.data} loop autoplay={hovered === p.id || active} style={{ width: 42, height: 42 }} />
                          </button>
                        </Tooltip>
                      );
                    })}
                  </div>
                </div>
              )}

              {step === 2 && (
                <div>
                  <h2 className="m-0 text-[20px] font-bold">What should {agentName} do?</h2>
                  <p className="mt-1.5 text-[13px] leading-[1.65] text-muted">
                    Its role, your context, how it should behave. This becomes the agent's standing brief.
                  </p>
                  <textarea
                    autoFocus
                    value={objective}
                    onChange={(e) => setObjective(e.target.value)}
                    rows={8}
                    placeholder="e.g. You are my operations lead. Keep my projects moving, delegate research to my other agents, and ask before anything destructive."
                    className={`${PROSE_CLS} mt-5`}
                  />
                </div>
              )}

              {step === 3 && (
                <div>
                  <h2 className="m-0 text-[20px] font-bold">Pick its model</h2>
                  <p className="mt-1.5 text-[13px] leading-[1.65] text-muted">The provider {agentName} thinks with. Changeable any time.</p>
                  {enabledModels.length === 0 ? (
                    <div className="mt-5 rounded-xl border border-danger/40 bg-surface px-4 py-3 text-[12px] leading-[1.65] text-muted">
                      No models are enabled yet. Finish here, then add one in <b className="text-foreground">Settings → Models</b> —
                      nothing can run until you do.
                    </div>
                  ) : (
                    <div className="mt-5 grid max-h-[300px] gap-1.5 overflow-x-hidden overflow-y-auto pr-1">
                      {enabledModels.map((m) => {
                        const active = m.id === model;
                        return (
                          <button
                            key={m.id}
                            type="button"
                            onClick={() => setModel(m.id)}
                            className={`focus-ring flex cursor-pointer items-center justify-between gap-3 rounded-lg border px-3.5 py-2.5 text-left transition-colors duration-150 ${
                              active ? 'border-foreground/40 bg-surface' : 'border-border hover:bg-surface'
                            }`}
                          >
                            <span className="min-w-0">
                              <b className="block truncate font-mono text-[11px] text-foreground">{m.label}</b>
                              <span className="block truncate font-mono text-[10px] text-muted">{m.id}</span>
                            </span>
                            <span className="shrink-0 font-mono text-[10px] tracking-wider text-muted uppercase">{m.provider}</span>
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      <footer className="relative z-10 flex shrink-0 items-center justify-between px-6 pb-6">
        <button className="focus-ring cursor-pointer rounded border-0 bg-transparent font-mono text-[11px] lowercase text-muted transition-colors hover:text-foreground" onClick={onSkip}>
          skip setup
        </button>
        <div className="flex items-center gap-2">
          {step > 0 && (
            <Button icon={<ArrowLeft size={13} />} onClick={() => setStep((s) => s - 1)} disabled={saving}>back</Button>
          )}
          {step > 0 && (last ? (
            <Button variant="accent" icon={<Check size={13} />} onClick={finish} disabled={saving}>
              {saving ? 'saving…' : `start with ${name.trim() || 'your agent'}`}
            </Button>
          ) : (
            <Button variant="accent" icon={<ArrowRight size={13} />} onClick={() => setStep((s) => s + 1)}>next</Button>
          ))}
        </div>
      </footer>
    </div>
  );
}
