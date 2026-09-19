import { useEffect, useState } from 'react';
import { ArrowLeft, ArrowRight, Check } from 'lucide-react';
import { Lottie } from 'lottie-react';
import type { Agent, ModelConfig } from '../types';
import { PERSONAS } from './ui/AgentAvatar';

export type OnboardingPatch = { name: string; objective: string; persona: string; model: string };

const STEPS = ['Welcome', 'Name', 'Purpose', 'Model'] as const;

const personaData = (id: string) => PERSONAS.find((p) => p.id === id)?.data ?? PERSONAS[0].data;

// A soft radial wash so the stage never sits on a flat background.
const GLOW = {
  background:
    'radial-gradient(46% 40% at 20% 16%, rgba(52,211,153,0.10), transparent 62%), radial-gradient(52% 46% at 84% 90%, rgba(124,148,204,0.10), transparent 62%)',
};
const STAGE_GLOW = {
  background: 'radial-gradient(50% 50% at 50% 50%, rgba(52,211,153,0.16), transparent 70%)',
  filter: 'blur(10px)',
};

// First-run setup. Renders at z-40 so the topbar and its window controls stay usable.
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
    <div className="fixed inset-0 z-40 flex flex-col overflow-hidden bg-[var(--color-bg)]">
      <div className="pointer-events-none absolute inset-0" style={GLOW} />

      {/* Progress — completed steps are clickable to go back */}
      <header className="relative z-10 flex shrink-0 flex-wrap items-center justify-center gap-2 px-6 pt-7">
        {STEPS.map((s, i) => (
          <button
            key={s}
            disabled={i > step}
            onClick={() => setStep(i)}
            className={`flex items-center gap-2 rounded-full border px-3 py-1.5 text-[11px] transition-colors duration-150 ${
              i === step ? 'border-[var(--green)] bg-panel2 text-text' : i < step ? 'border-line text-muted hover:border-mid hover:text-text' : 'border-transparent text-mid'
            }`}
          >
            <span className={`grid h-4 w-4 place-items-center rounded-full text-[9px] font-bold ${i <= step ? 'bg-[var(--green)] text-[var(--color-bg)]' : 'bg-line text-muted'}`}>{i + 1}</span>
            {s}
          </button>
        ))}
      </header>

      <div className="relative z-10 flex min-h-0 flex-1 items-center justify-center px-6 py-6">
        {step === 0 ? (
          <div className="flex animate-[step-in_280ms_ease-out] flex-col items-center text-center">
            <div className="relative mb-4 grid h-[240px] w-[240px] place-items-center">
              <div className="absolute inset-0 rounded-full" style={STAGE_GLOW} />
              <Lottie src={personaData(persona)} loop autoplay style={{ width: 210, height: 210 }} />
            </div>
            <h1 className="m-0 max-w-[620px] text-[30px] leading-[1.15]">Your workspace, run by an agent you shape</h1>
            <p className="mt-3 max-w-[540px] text-[13px] leading-[1.8] text-muted">
              Everything runs on this machine — agents, tools, chats and runs stay local. One lead agent
              orchestrates the rest, and it's yours to name and direct.
            </p>
            <button className="primary mt-7" onClick={() => setStep(1)}>
              Get started<ArrowRight size={13} />
            </button>
          </div>
        ) : (
          <div key={step} className="grid w-full max-w-[980px] animate-[step-in_220ms_ease-out] grid-cols-1 items-start gap-10 lg:grid-cols-[300px_minmax(0,1fr)]">
            {/* Live stage — reflects the choices as they're made */}
            <div className="flex flex-col items-center">
              <div className="relative grid h-[210px] w-[210px] place-items-center">
                <div className="absolute inset-0 rounded-full" style={STAGE_GLOW} />
                <Lottie src={personaData(persona)} loop autoplay style={{ width: 185, height: 185 }} />
              </div>
              <b className="mt-1 max-w-[260px] truncate text-[15px]">{name.trim() || 'Your lead agent'}</b>
              <span className="mt-1 max-w-[260px] truncate font-mono text-[10.5px] text-muted">{pickedModel?.label ?? 'no model selected'}</span>
            </div>

            <div className="min-w-0">
              {step === 1 && (
                <div>
                  <h2 className="m-0 text-[20px]">Name your lead agent</h2>
                  <p className="mt-1.5 text-[12.5px] leading-[1.7] text-muted">Call it whatever you like — this is who you'll be talking to.</p>
                  <input
                    autoFocus
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="Chief, Ada, Boss…"
                    className="mt-5 w-full max-w-[440px] rounded-xl border border-line bg-panel2 px-3.5 py-3 text-[14px] text-text outline-none transition-colors focus:border-mid"
                  />
                  <label className="mt-6 mb-2 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">Look</label>
                  <div className="grid max-w-[440px] grid-cols-5 gap-2 sm:grid-cols-6">
                    {PERSONAS.map((p) => {
                      const active = p.id === persona;
                      return (
                        <button
                          key={p.id}
                          type="button"
                          title={p.label}
                          onClick={() => setPersona(p.id)}
                          onMouseEnter={() => setHovered(p.id)}
                          onMouseLeave={() => setHovered((h) => (h === p.id ? null : h))}
                          className={`grid cursor-pointer place-items-center rounded-xl border p-1.5 transition-colors duration-150 ${active ? 'border-[var(--green)] bg-panel2' : 'border-line hover:border-mid'}`}
                        >
                          <Lottie src={p.data} loop autoplay={hovered === p.id || active} style={{ width: 42, height: 42 }} />
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}

              {step === 2 && (
                <div>
                  <h2 className="m-0 text-[20px]">What should {agentName} do?</h2>
                  <p className="mt-1.5 text-[12.5px] leading-[1.7] text-muted">
                    Its role, your context, how it should behave. This becomes the agent's standing brief.
                  </p>
                  <textarea
                    autoFocus
                    value={objective}
                    onChange={(e) => setObjective(e.target.value)}
                    rows={8}
                    placeholder="e.g. You are my operations lead. Keep my projects moving, delegate research to my other agents, and ask before anything destructive."
                    className="mt-5 w-full resize-none rounded-xl border border-line bg-panel2 px-3.5 py-3 text-[13.5px] leading-[1.75] text-text outline-none transition-colors focus:border-mid"
                  />
                </div>
              )}

              {step === 3 && (
                <div>
                  <h2 className="m-0 text-[20px]">Pick its model</h2>
                  <p className="mt-1.5 text-[12.5px] leading-[1.7] text-muted">The provider {agentName} thinks with. Changeable any time.</p>
                  {enabledModels.length === 0 ? (
                    <div className="mt-5 rounded-xl border border-[#facc15]/40 bg-panel2 px-4 py-3 text-[12px] leading-[1.7] text-muted">
                      No models are enabled yet. Finish here, then add one in <b className="text-text">Settings → Models</b> —
                      nothing can run until you do.
                    </div>
                  ) : (
                    <div className="mt-5 grid max-h-[300px] gap-2 overflow-y-auto pr-1">
                      {enabledModels.map((m) => {
                        const active = m.id === model;
                        return (
                          <button
                            key={m.id}
                            type="button"
                            onClick={() => setModel(m.id)}
                            className={`flex cursor-pointer items-center justify-between gap-3 rounded-xl border px-3.5 py-3 text-left transition-colors duration-150 ${active ? 'border-[var(--green)] bg-panel2' : 'border-line hover:border-mid'}`}
                          >
                            <span className="min-w-0">
                              <b className="block truncate text-[13px]">{m.label}</b>
                              <span className="block truncate font-mono text-[10.5px] text-muted">{m.id}</span>
                            </span>
                            <span className="shrink-0 font-mono text-[10px] uppercase tracking-[0.08em] text-muted">{m.provider}</span>
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

      <footer className="relative z-10 flex shrink-0 items-center justify-between px-6 pb-7">
        <button className="cursor-pointer border-0 bg-transparent p-0 text-[12px] text-muted transition-colors hover:text-text" onClick={onSkip}>
          Skip setup
        </button>
        <div className="flex items-center gap-2">
          {step > 0 && (
            <button className="secondary" onClick={() => setStep((s) => s - 1)} disabled={saving}>
              <ArrowLeft size={13} />Back
            </button>
          )}
          {step > 0 && (last ? (
            <button className="primary" onClick={finish} disabled={saving}>
              <Check size={13} />{saving ? 'Saving…' : `Start with ${name.trim() || 'your agent'}`}
            </button>
          ) : (
            <button className="primary" onClick={() => setStep((s) => s + 1)}>
              Next<ArrowRight size={13} />
            </button>
          ))}
        </div>
      </footer>
    </div>
  );
}
