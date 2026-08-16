import { Lottie } from 'lottie-react';
import gremlin from '../assets/agents/gremlin.json';

// Persona registry: each persona maps to a Lottie gremlin animation file.
// Drop new .json files in src/assets/agents/ and add them here so the config
// picker and avatars can use them.
export const PERSONAS: { id: string; label: string; data: string | object }[] = [
  { id: 'gremlin', label: 'Gremlin', data: gremlin },
  // e.g. { id: 'researcher', label: 'Researcher Gremlin', data: researcher },
];

const personaData = (id?: string) => PERSONAS.find((p) => p.id === id)?.data;

export function AgentAvatar({ agent, size = 32, animate = true }: {
  agent: { id?: string; name: string; color?: string; persona?: string };
  size?: number;
  animate?: boolean;
}) {
  const data = animate ? personaData(agent.persona) ?? personaData('gremlin') : undefined;

  if (data) {
    return (
      <span
        className="grid shrink-0 place-items-center overflow-hidden rounded-lg bg-panel2"
        style={{ width: size, height: size }}
      >
        <Lottie src={data} loop autoplay style={{ width: size * 1.4, height: size * 1.4 }} />
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
