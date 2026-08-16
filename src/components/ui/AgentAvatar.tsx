import { Lottie } from 'lottie-react';
import { useEffect, useRef, useState } from 'react';
import { Check, ChevronRight } from 'lucide-react';
import aiOrb from '../../assets/agents/ai-orb-persona.json';
import astronaut from '../../assets/agents/astronaut-persona.json';
import cowboyRobot from '../../assets/agents/cowboy-robot-persona.json';
import cyberpunkHacker from '../../assets/agents/cyberpunk-hacker-persona.json';
import forestRanger from '../../assets/agents/forest-ranger-persona.json';
import medievalKnight from '../../assets/agents/medieval-knight-persona.json';
import medievalQueen from '../../assets/agents/medieval-queen-persona.json';
import pirate from '../../assets/agents/pirate-persona.json';
import scientist from '../../assets/agents/scientist-persona.json';
import wizard from '../../assets/agents/wizard-persona.json';

// Persona registry: each persona maps to a Lottie gremlin animation file.
// Drop new .json files in src/assets/agents/ and add them here so the config
// picker and avatars can use them.
export const PERSONAS: { id: string; label: string; data: string | object }[] = [
  { id: 'ai-orb', label: 'AI Orb', data: aiOrb },
  { id: 'astronaut', label: 'Astronaut', data: astronaut },
  { id: 'cowboy-robot', label: 'Cowboy Robot', data: cowboyRobot },
  { id: 'cyberpunk-hacker', label: 'Cyberpunk Hacker', data: cyberpunkHacker },
  { id: 'forest-ranger', label: 'Forest Ranger', data: forestRanger },
  { id: 'medieval-knight', label: 'Medieval Knight', data: medievalKnight },
  { id: 'medieval-queen', label: 'Medieval Queen', data: medievalQueen },
  { id: 'pirate', label: 'Pirate', data: pirate },
  { id: 'scientist', label: 'Scientist', data: scientist },
  { id: 'wizard', label: 'Wizard', data: wizard },
];

const personaData = (id?: string) => PERSONAS.find((p) => p.id === id)?.data ?? PERSONAS[0].data;

export function AgentAvatar({ agent, size = 32, animate = true, playing }: {
  agent: { id?: string; name: string; color?: string; persona?: string };
  size?: number;
  animate?: boolean;
  playing?: boolean;
}) {
  const [hovered, setHovered] = useState(false);
  const data = animate ? personaData(agent.persona) : undefined;

  if (data) {
    const active = playing ?? hovered;
    return (
      <span
        className="grid shrink-0 place-items-center overflow-hidden rounded-lg bg-panel2"
        style={{ width: size, height: size }}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        <Lottie src={data} loop autoplay={active} style={{ width: size * 1.4, height: size * 1.4 }} />
      </span>
    );
  }

  // Fallback: colored initial tile.
  return (
    <span
      className="grid shrink-0 place-items-center rounded-lg bg-panel2 text-[13px] font-bold"
      style={{ width: size, height: size, color: agent.color }}
    >
      {agent.name.charAt(0).toUpperCase()}
    </span>
  );
}

// A dropdown that shows every persona as a live animated preview, so you can
// see the wiggles before picking one.
export function PersonaPicker({ value, onChange }: {
  value: string;
  onChange: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [hovered, setHovered] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const selected = PERSONAS.find((p) => p.id === value) ?? PERSONAS[0];

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <button type="button" className="group flex w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-line bg-panel2 px-3 py-2.5 text-left text-[13px] text-text hover:border-mid" onClick={() => setOpen((o) => !o)}>
        <span className="flex items-center gap-2.5">
          <span className="grid h-7 w-7 shrink-0 place-items-center overflow-hidden rounded-md bg-panel"><Lottie src={selected.data} loop autoplay style={{ width: 28, height: 28 }} /></span>
          {selected.label}
        </span>
        <ChevronRight size={13} className="rotate-90 text-muted transition-transform duration-150 group-aria-expanded:-rotate-90" />
      </button>

      {open && (
        <div className="absolute top-[calc(100%+6px)] left-0 right-0 z-30 grid max-h-[420px] min-w-[320px] grid-cols-3 gap-1 overflow-y-auto rounded-lg border border-line bg-panel2 p-2 shadow-[0_14px_34px_#000a]">
          {PERSONAS.map((p) => {
            const active = p.id === value;
            // Every tile always shows the persona (first frame); hover plays the wiggle.
            return (
              <button
                key={p.id}
                type="button"
                className={`flex cursor-pointer flex-col items-center gap-1 rounded-[6px] border p-2 text-center transition-colors ${active ? 'border-mid bg-line' : 'border-transparent hover:bg-line'}`}
                onClick={() => { onChange(p.id); setOpen(false); }}
                onMouseEnter={() => setHovered(p.id)}
                onMouseLeave={() => setHovered((h) => (h === p.id ? null : h))}
              >
                <span className="grid h-12 w-12 place-items-center overflow-hidden rounded-lg bg-panel">
                  <Lottie src={p.data} loop autoplay={hovered === p.id || active} style={{ width: 48, height: 48 }} />
                </span>
                <span className="flex items-center gap-1 text-[11px] text-text">{p.label}{active && <Check size={11} className="text-[var(--green)]" />}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
