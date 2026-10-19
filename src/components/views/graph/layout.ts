import type { Agent, Integration, Project, Skill, Tool, Workflow } from '../../../types';
import type { GEdge, GNode } from './shared';

// One memory scope that holds something. `am_list_scopes` returns this shape (see
// the Memory view); the graph reads it rather than the namespace registry.
export type MemoryScope = {
  namespace: { key: string; type: string; id: string | null; name: string; description?: string | null };
  counts: { events: number; memories: number; selfModelEntries: number };
  lastActivityAt?: number | null;
};

export type GraphInput = {
  agents: Agent[]; skills: Skill[]; tools: Tool[]; integrations: Integration[];
  workflows: Workflow[]; projects: Project[]; scopes: MemoryScope[];
};

// Everything in the workspace as one graph: what exists, and what is wired to
// what. Agents are the hub — capabilities hang off them, workflows consume them,
// and memory scopes attach to whatever owns them.
export function buildGraph({ agents, skills, tools, integrations, workflows, projects, scopes }: GraphInput): { nodes: GNode[]; edges: GEdge[] } {
  const nodes: GNode[] = [];
  skills.forEach((s) => nodes.push({ id: `skill:${s.id}`, kind: 'skill', label: s.name, sub: 'instructions' }));
  tools.forEach((t) => nodes.push({ id: `tool:${t.id}`, kind: 'tool', label: t.name, sub: t.kind.replace('_', ' ') }));
  integrations.filter((i) => i.connected).forEach((i) => nodes.push({ id: `int:${i.id}`, kind: 'integration', label: i.name, sub: `${i.actions.length} actions` }));
  agents.forEach((a) => nodes.push({
    id: `agent:${a.id}`, kind: a.isManager ? 'manager' : 'agent', label: a.name,
    sub: a.isManager ? 'system agent' : `${(a.toolIds ?? []).length} tools · ${(a.skillIds ?? []).length} skills`,
  }));
  workflows.forEach((w) => nodes.push({ id: `wf:${w.id}`, kind: 'workflow', label: w.name, sub: `${w.nodes.length} nodes` }));
  projects.forEach((p) => nodes.push({ id: `project:${p.id}`, kind: 'project', label: p.name, sub: p.description || 'project' }));
  // Accumulated memory, one glob per scope, sized by how much it holds.
  scopes.forEach((s) => {
    const c = s.counts;
    nodes.push({
      id: `mem:${s.namespace.key}`, kind: 'memory',
      label: s.namespace.name || s.namespace.key,
      sub: `${c.memories} memories · ${c.events} events`,
      weight: c.memories + c.events + c.selfModelEntries,
    });
  });

  const known = new Set(nodes.map((n) => n.id));
  const edges: GEdge[] = [];
  const link = (from: string, to: string, kind: GEdge['kind']) => {
    if (from === to) return;
    if (known.has(from) && known.has(to)) edges.push({ id: `${kind}:${from}->${to}`, from, to, kind });
  };

  const CAP_FIELDS: ('skillIds' | 'toolIds' | 'integrations')[] = ['skillIds', 'toolIds', 'integrations'];
  const PREFIX: Record<string, string> = { skillIds: 'skill', toolIds: 'tool', integrations: 'int' };
  agents.forEach((a) => CAP_FIELDS.forEach((field) => {
    (a[field] ?? []).forEach((id) => link(`${PREFIX[field]}:${id}`, `agent:${a.id}`, 'uses'));
  }));
  // A generic API tool declares the integration it calls through.
  tools.forEach((t) => { if (t.integrationId) link(`tool:${t.id}`, `int:${t.integrationId}`, 'derives'); });
  workflows.forEach((w) => w.nodes.forEach((n) => { if (n.agentId) link(`agent:${n.agentId}`, `wf:${w.id}`, 'flow'); }));
  // A memory scope attaches to its owner; a global scope floats free at the centre.
  const hasGlobal = scopes.some((s) => s.namespace.key === 'global');
  scopes.forEach((s) => {
    const key = s.namespace.key;
    if (key.startsWith('agent:')) link(`agent:${key.slice('agent:'.length)}`, `mem:${key}`, 'derives');
    else if (key.startsWith('project:')) link(`project:${key.slice('project:'.length)}`, `mem:${key}`, 'derives');
    // Every scope is read together with global (memory inherits through it), so
    // the tie is drawn even though nothing owns global directly.
    if (hasGlobal && key !== 'global') link(`mem:${key}`, 'mem:global', 'derives');
  });

  return { nodes, edges };
}

export type Vec = [number, number, number];

// A one-shot force simulation: springs along the edges, repulsion between
// everything, and a gentle pull to the centre. Deterministic, so the same graph
// always settles into the same shape instead of jittering on every render.
export function layout(nodes: GNode[], edges: GEdge[], dims: 2 | 3, iterations = 260): Map<string, Vec> {
  const out = new Map<string, Vec>();
  const n = nodes.length;
  if (!n) return out;

  // Seed on a Fibonacci sphere (a spiral for 2D) for an even starting spread.
  const spread = 5 + n * 0.4;
  const pts: Vec[] = nodes.map((_, i) => {
    const t = (i + 0.5) / n;
    if (dims === 2) {
      const a = i * 2.399963229728653;
      const r = spread * Math.sqrt(t);
      return [Math.cos(a) * r, Math.sin(a) * r, 0];
    }
    const y = 1 - 2 * t;
    const r = Math.sqrt(Math.max(0, 1 - y * y));
    const phi = i * 2.399963229728653;
    return [Math.cos(phi) * r * spread, y * spread, Math.sin(phi) * r * spread];
  });

  const idx = new Map(nodes.map((nd, i) => [nd.id, i]));
  const links = edges
    .map((e) => [idx.get(e.from), idx.get(e.to)] as const)
    .filter((p): p is readonly [number, number] => p[0] !== undefined && p[1] !== undefined);

  const REST = dims === 2 ? 11 : 9;
  const REPULSION = dims === 2 ? 42 : 34;

  for (let step = 0; step < iterations; step++) {
    const cool = 1 - step / iterations;
    const disp: Vec[] = pts.map(() => [0, 0, 0]);
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        let dx = pts[i][0] - pts[j][0];
        let dy = pts[i][1] - pts[j][1];
        let dz = dims === 2 ? 0 : pts[i][2] - pts[j][2];
        let d2 = dx * dx + dy * dy + dz * dz;
        if (d2 < 0.01) { dx = (i - j) * 0.01 || 0.01; dy = 0.01; d2 = dx * dx + 0.0001; }
        const d = Math.sqrt(d2);
        const f = REPULSION / d2;
        disp[i][0] += (dx / d) * f; disp[i][1] += (dy / d) * f; disp[i][2] += (dz / d) * f;
        disp[j][0] -= (dx / d) * f; disp[j][1] -= (dy / d) * f; disp[j][2] -= (dz / d) * f;
      }
    }
    for (const [a, b] of links) {
      const dx = pts[b][0] - pts[a][0];
      const dy = pts[b][1] - pts[a][1];
      const dz = dims === 2 ? 0 : pts[b][2] - pts[a][2];
      const d = Math.max(0.1, Math.sqrt(dx * dx + dy * dy + dz * dz));
      const f = (d - REST) * 0.06;
      disp[a][0] += (dx / d) * f; disp[a][1] += (dy / d) * f; disp[a][2] += (dz / d) * f;
      disp[b][0] -= (dx / d) * f; disp[b][1] -= (dy / d) * f; disp[b][2] -= (dz / d) * f;
    }
    const k = 0.9 * cool + 0.1;
    for (let i = 0; i < n; i++) {
      const p = pts[i];
      p[0] += disp[i][0] * k - p[0] * 0.006;
      p[1] += disp[i][1] * k - p[1] * 0.006;
      if (dims === 3) p[2] += disp[i][2] * k - p[2] * 0.006;
    }
  }

  nodes.forEach((nd, i) => out.set(nd.id, pts[i]));
  return out;
}
