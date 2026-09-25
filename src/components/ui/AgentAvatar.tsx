import { Lottie } from 'lottie-react';
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, ChevronDown } from 'lucide-react';
import { useAnchoredPosition, useDismiss } from './popover';
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
import browserAgent from '../../assets/agents/browser-agent.json';
import coderAgent from '../../assets/agents/coder-agent.json';
import creativeAgent from '../../assets/agents/creative-agent.json';
import dataAgent from '../../assets/agents/data-agent.json';
import managerAgent from '../../assets/agents/manager-agent.json';
import plannerAgent from '../../assets/agents/planner-agent.json';
import researcherAgent from '../../assets/agents/researcher-agent.json';
import scoutAgent from '../../assets/agents/scout-agent.json';
import securityAgent from '../../assets/agents/security-agent.json';
import writerAgent from '../../assets/agents/writer-agent.json';

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
  { id: 'browser', label: 'Browser', data: browserAgent },
  { id: 'coder', label: 'Coder', data: coderAgent },
  { id: 'creative', label: 'Creative', data: creativeAgent },
  { id: 'data', label: 'Data', data: dataAgent },
  { id: 'manager', label: 'Manager', data: managerAgent },
  { id: 'planner', label: 'Planner', data: plannerAgent },
  { id: 'researcher', label: 'Researcher', data: researcherAgent },
  { id: 'scout', label: 'Scout', data: scoutAgent },
  { id: 'security', label: 'Security', data: securityAgent },
  { id: 'writer', label: 'Writer', data: writerAgent },
];

const personaData = (id?: string) => PERSONAS.find((p) => p.id === id)?.data ?? PERSONAS[0].data;

export function AgentAvatar({ agent, size = 32, animate = true, playing, fluid = false }: {
  agent: { id?: string; name: string; color?: string; persona?: string; avatar?: string };
  size?: number;
  animate?: boolean;
  playing?: boolean;
  // Fills the parent's width as a square — used by the agent tiles.
  fluid?: boolean;
}) {
  const [hovered, setHovered] = useState(false);
  const boxClass = fluid
    ? 'grid aspect-square w-full place-items-center overflow-hidden rounded-lg bg-background'
    : 'grid shrink-0 place-items-center overflow-hidden rounded-lg bg-background';
  const boxStyle = fluid ? undefined : { width: size, height: size };

  // A custom uploaded avatar wins over the persona animation.
  if (agent.avatar) {
    return (
      <span className={boxClass} style={boxStyle}>
        <img src={agent.avatar} alt={agent.name} className="h-full w-full object-cover" />
      </span>
    );
  }

  const data = animate ? personaData(agent.persona) : undefined;

  if (data) {
    const active = playing ?? hovered;
    return (
      <span
        className={boxClass}
        style={boxStyle}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        <Lottie src={data} loop autoplay={active} style={fluid ? { width: '135%', height: '135%' } : { width: size * 1.4, height: size * 1.4 }} />
      </span>
    );
  }

  // Fallback: neutral initial tile.
  return (
    <span
      className={`grid place-items-center rounded-lg bg-background text-muted ${fluid ? 'aspect-square w-full text-[28px]' : 'shrink-0 text-[13px]'}`}
      style={fluid ? undefined : { width: size, height: size }}
    >
      {agent.name.charAt(0).toUpperCase()}
    </span>
  );
}

// A dropdown that shows every persona as a live animated preview, so you can
// see the wiggles before picking one. Same trigger shell and same portalled
// popover panel as Select — one grammar per pattern.
export function PersonaPicker({ value, onChange }: {
  value: string;
  onChange: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [hovered, setHovered] = useState<string | null>(null);
  const anchor = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const pos = useAnchoredPosition(open, anchor, { minWidth: 330 });
  useDismiss(open, () => setOpen(false), anchor, panel);
  const selected = PERSONAS.find((p) => p.id === value) ?? PERSONAS[0];

  useEffect(() => { if (!open) setHovered(null); }, [open]);

  return (
    <div className="relative" ref={anchor}>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="focus-ring flex h-[38px] w-full cursor-pointer items-center justify-between gap-2 rounded-lg border border-border bg-surface px-2.5 text-left font-mono text-[11px] text-foreground transition-colors hover:bg-background"
      >
        <span className="flex min-w-0 items-center gap-2.5">
          <span className="grid h-6 w-6 shrink-0 place-items-center overflow-hidden rounded bg-background">
            <Lottie src={selected.data} loop autoplay style={{ width: 24, height: 24 }} />
          </span>
          <span className="min-w-0 truncate">{selected.label}</span>
        </span>
        <ChevronDown size={13} className={`shrink-0 text-muted transition-transform duration-150 ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && pos && createPortal(
        <div
          ref={panel}
          role="listbox"
          className="popover-shell grid w-[330px] max-h-[420px] grid-cols-3 gap-1 overflow-x-hidden overflow-y-auto p-2"
          style={{ top: pos.top, bottom: pos.bottom, left: pos.left }}
        >
          {PERSONAS.map((p) => {
            const active = p.id === value;
            // Every tile always shows the persona (first frame); hover plays the wiggle.
            return (
              <button
                key={p.id}
                type="button"
                role="option"
                aria-selected={active}
                className={`flex cursor-pointer flex-col items-center gap-1 rounded-lg border p-1.5 text-center transition-colors ${
                  active ? 'border-foreground/40 bg-background' : 'border-transparent hover:bg-background'
                }`}
                onClick={() => { onChange(p.id); setOpen(false); }}
                onMouseEnter={() => setHovered(p.id)}
                onMouseLeave={() => setHovered((h) => (h === p.id ? null : h))}
              >
                <span className="grid h-12 w-12 place-items-center overflow-hidden rounded bg-surface">
                  <Lottie src={p.data} loop autoplay={hovered === p.id || active} style={{ width: 48, height: 48 }} />
                </span>
                <span className="flex items-center gap-1 font-mono text-[10px] text-muted">
                  <span className="min-w-0 truncate">{p.label}</span>
                  {active && <Check size={10} className="shrink-0 text-foreground" />}
                </span>
              </button>
            );
          })}
        </div>,
        document.body,
      )}
    </div>
  );
}
