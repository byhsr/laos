import { useState } from 'react';
import { ArrowRight, Check, Sparkles } from 'lucide-react';
import type { Agent, ModelConfig } from '../types';
import { AgentAvatar, PersonaPicker } from './ui/AgentAvatar';
import { Dropdown } from './ui/Dropdown';

export type OnboardingPatch = { name: string; objective: string; persona: string; model: string };

const STEPS = ['Welcome', 'Name', 'Purpose', 'Model'] as const;

// First-run setup: names the lead agent and tells it what the user wants.
// Deliberately sits below the topbar (z-40 vs z-50) so the window controls stay
// reachable.
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
  const [saving, setSaving] = useState(false);

  const enabledModels = models.filter((m) => m.enabled);
  const last = step === STEPS.length - 1;

  const finish = async () => {
    setSaving(true);
    try {
      await onFinish({ name: name.trim() || 'Manager', objective: objective.trim(), persona, model });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/70 p-6">
      <div className="glass-strong w-full max-w-[560px] rounded-3xl border border-hairline p-8 shadow-float">
        {/* Step rail */}
        <div className="mb-6 flex items-center gap-2">
          {STEPS.map((s, i) => (
            <div key={s} className="flex items-center gap-2">
              <span className={`h-1.5 w-1.5 rounded-full ${i <= step ? 'bg-[var(--green)]' : 'bg-line'}`} />
              {i < STEPS.length - 1 && <span className={`h-px w-6 ${i < step ? 'bg-[var(--green)]' : 'bg-line'}`} />}
            </div>
          ))}
        </div>

        {step === 0 && (
          <div>
            <div className="mb-3 flex items-center gap-2">
              <Sparkles size={15} className="text-[var(--green)]" />
              <h2 className="m-0 text-[18px]">Welcome to your workspace</h2>
            </div>
            <p className="text-[12.5px] leading-[1.75] text-muted">
              This app runs agents locally on your machine. Everything — agents, tools, chats and runs —
              stays on this device.
            </p>
            <p className="mt-3 text-[12.5px] leading-[1.75] text-muted">
              At the top sits your lead agent. It orchestrates the others, creates and runs work, and is
              the one you talk to first. Let's set it up.
            </p>
          </div>
        )}

        {step === 1 && (
          <div>
            <h2 className="m-0 text-[18px]">Name your lead agent</h2>
            <p className="mt-1.5 text-[12.5px] text-muted">Call it whatever you like — this is who you'll be talking to.</p>

            <label className="mt-5 mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">NAME</label>
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Chief, Ada, Boss…"
              className="w-full rounded-md border border-line bg-panel2 px-3 py-2.5 text-[13px] text-text outline-none focus:border-mid"
            />

            <label className="mt-4 mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">LOOK</label>
            <PersonaPicker value={persona} onChange={setPersona} />
            <div className="mt-3 flex items-center gap-3">
              <AgentAvatar agent={{ ...manager, name: name || 'Manager', persona }} size={44} playing />
              <span className="text-[12px] text-muted">{name.trim() || 'Your lead agent'}</span>
            </div>
          </div>
        )}

        {step === 2 && (
          <div>
            <h2 className="m-0 text-[18px]">What should it do?</h2>
            <p className="mt-1.5 text-[12.5px] text-muted">
              Describe how you want it to work — its role, your context, how it should behave.
            </p>
            <textarea
              autoFocus
              value={objective}
              onChange={(e) => setObjective(e.target.value)}
              rows={6}
              placeholder="e.g. You are my operations lead. Keep my projects moving, delegate research to my agents, and check with me before anything destructive."
              className="mt-4 w-full resize-y rounded-md border border-line bg-panel2 px-3 py-2.5 text-[13px] leading-[1.7] text-text outline-none focus:border-mid"
            />
          </div>
        )}

        {step === 3 && (
          <div>
            <h2 className="m-0 text-[18px]">Pick its model</h2>
            <p className="mt-1.5 text-[12.5px] text-muted">The provider this agent thinks with. You can change it any time.</p>

            {enabledModels.length === 0 ? (
              <div className="mt-4 rounded-lg border border-[#facc15]/40 bg-panel2 px-3 py-2.5 text-[12px] leading-[1.6] text-muted">
                No models are enabled yet. Finish here, then add one in <b className="text-text">Settings → Models</b> —
                nothing can run until you do.
              </div>
            ) : (
              <>
                <label className="mt-5 mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">MODEL</label>
                <Dropdown
                  value={model}
                  options={enabledModels.map((m) => ({ value: m.id, label: m.label }))}
                  onChange={setModel}
                />
              </>
            )}
          </div>
        )}

        <div className="mt-8 flex items-center justify-between gap-3">
          <button className="cursor-pointer border-0 bg-transparent p-0 text-[12px] text-muted hover:text-text" onClick={onSkip}>
            Skip setup
          </button>
          <div className="flex items-center gap-2">
            {step > 0 && (
              <button className="secondary" onClick={() => setStep((s) => s - 1)} disabled={saving}>Back</button>
            )}
            {last ? (
              <button className="primary" onClick={finish} disabled={saving}>
                <Check size={13} />{saving ? 'Saving…' : `Start with ${name.trim() || 'your agent'}`}
              </button>
            ) : (
              <button className="primary" onClick={() => setStep((s) => s + 1)}>
                Next<ArrowRight size={13} />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
