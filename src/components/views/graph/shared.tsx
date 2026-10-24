import type { ReactNode } from 'react';
import { ArrowUpRight, Bot, Brain, FolderKanban, Globe, Network, Plus, Sparkles, UserCog, Workflow as WorkflowIcon, Wrench, X } from 'lucide-react';
import type { Agent, Integration, Project, Skill, Tool, Workflow } from '../../../types';
import { IconButton } from '../../ui/Button';
import { Tooltip } from '../../ui/Tooltip';
import { GROUP_LABEL_CLS } from '../../ui/Input';

// Every kind of thing the graph can show, and the palette both renderers use.
export type Kind = 'skill' | 'tool' | 'integration' | 'agent' | 'manager' | 'workflow' | 'project' | 'memory' | 'brain';

export type GNode = {
  id: string; kind: Kind; label: string; sub: string; weight?: number;
  // Brain-map extras (only on `brain` nodes): its own kind, scope, provenance and
  // attention signals, so the graph can filter and run the attention lens.
  brainKind?: string; scope?: string; provenance?: string;
  attention?: Record<string, number>; salience?: number;
  // Render overrides set by the attention/salience lens (radius + heat colour).
  r?: number; color?: string;
};
export type GEdge = { id: string; from: string; to: string; kind: 'uses' | 'flow' | 'derives' };

export type LinkField = 'toolIds' | 'skillIds' | 'integrations';

export const ICON: Record<Kind, ReactNode> = {
  skill: <Sparkles size={13} />,
  tool: <Wrench size={13} />,
  integration: <Globe size={13} />,
  agent: <Bot size={13} />,
  manager: <UserCog size={13} />,
  workflow: <WorkflowIcon size={13} />,
  project: <FolderKanban size={13} />,
  memory: <Brain size={13} />,
  brain: <Network size={13} />,
};

export const LABEL: Record<Kind, string> = {
  skill: 'Skills',
  tool: 'Tools',
  integration: 'Integrations',
  agent: 'Agents',
  manager: 'Manager',
  workflow: 'Workflows',
  project: 'Projects',
  memory: 'Memory',
  brain: 'Mind',
};

// One hue per kind, in the same desaturated register as the port tints, so the
// 3D scene reads without breaking the mono-chrome language.
export const NODE_COLOR: Record<Kind, string> = {
  agent: '#c9c9c9',
  manager: '#d7ff38',
  skill: '#b09a70',
  tool: '#7d9ec4',
  integration: '#7dae85',
  workflow: '#c07f7f',
  project: '#a89ec4',
  memory: '#7fc0b8',
  brain: '#b0a08f',
};

// Which agent field a capability node links through.
export const FIELD_OF: Partial<Record<Kind, LinkField>> = {
  tool: 'toolIds',
  skill: 'skillIds',
  integration: 'integrations',
};

export function LinkRow({ name, onRemove }: { name: string; onRemove: () => void }) {
  return (
    <div className="group flex items-center gap-2 rounded-lg border border-border bg-background px-2.5 py-1.5">
      <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{name}</span>
      <span className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100">
        <IconButton label="remove link" className="h-5 w-5 hover:bg-danger hover:text-white" onClick={onRemove}><X size={10} /></IconButton>
      </span>
    </div>
  );
}

export function LinkSection({ title, linked, available, onAdd, onRemove, empty }: {
  title: string;
  linked: { id: string; name: string }[];
  available: { id: string; name: string }[];
  onAdd: (id: string) => void;
  onRemove: (id: string) => void;
  empty: string;
}) {
  return (
    <section className="mb-5 last:mb-0">
      <div className="mb-2 flex items-baseline justify-between gap-2">
        <span className={GROUP_LABEL_CLS}>{title}</span>
        <span className="font-mono text-[10px] text-muted">{linked.length}</span>
      </div>

      {linked.length === 0 && available.length === 0 ? (
        <p className="m-0 font-mono text-[10px] leading-relaxed text-muted">{empty}</p>
      ) : (
        <div className="grid gap-1.5">
          {linked.map((i) => <LinkRow key={i.id} name={i.name} onRemove={() => onRemove(i.id)} />)}
          {available.length > 0 && (
            <div className="mt-0.5 flex flex-wrap gap-1.5">
              {available.map((i) => (
                <Tooltip key={i.id} label={`link ${i.name}`}>
                  <button
                    className="focus-ring flex max-w-full cursor-pointer items-center gap-1 rounded-lg border border-border px-2 py-1 font-mono text-[10px] text-muted transition-colors hover:border-foreground/30 hover:text-foreground"
                    onClick={() => onAdd(i.id)}
                  >
                    <Plus size={10} /><span className="truncate">{i.name}</span>
                  </button>
                </Tooltip>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

// The right-hand panel for a selected node. Identical in both renderers — it is
// the one place links between an agent and its capabilities are edited.
export function Inspector({ node, agents, skills, tools, integrations, workflows, projects, onClose, onOpenAgent, onOpenWorkflow, onSaveAgent, onSaveWorkflow }: {
  node: GNode;
  agents: Agent[]; skills: Skill[]; tools: Tool[]; integrations: Integration[]; workflows: Workflow[]; projects: Project[];
  onClose: () => void; onOpenAgent: (id: string) => void; onOpenWorkflow: (id: string) => void;
  onSaveAgent: (a: Agent) => Promise<void>; onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
}) {
  const named = (list: { id: string; name: string }[]) => list.map((i) => ({ id: i.id, name: i.name }));
  const namedAgents = named(agents);

  const patchAgent = (agent: Agent, field: LinkField, next: string[]) => {
    void onSaveAgent({ ...agent, [field]: next });
  };

  const openTarget = () => {
    const rawId = node.id.slice(node.id.indexOf(':') + 1);
    if (node.kind === 'agent' || node.kind === 'manager') onOpenAgent(rawId);
    if (node.kind === 'workflow') onOpenWorkflow(rawId);
  };

  const isAgentish = node.kind === 'agent' || node.kind === 'manager';
  const agentNode = isAgentish ? agents.find((a) => `agent:${a.id}` === node.id) : undefined;
  const wfNode = node.kind === 'workflow' ? workflows.find((w) => `wf:${w.id}` === node.id) : undefined;
  const capField = FIELD_OF[node.kind];
  // A memory node's scope is a compact key: agent:<id>, project:<id> or global.
  const memKey = node.kind === 'memory' ? node.id.slice(node.id.indexOf(':') + 1) : '';

  return (
    <aside className="flex w-[300px] shrink-0 flex-col overflow-hidden rounded-xl border border-border bg-surface">
      <header className="flex shrink-0 items-start justify-between gap-2 border-b border-border px-3.5 py-3">
        <div className="min-w-0 flex-1">
          <span className={`${GROUP_LABEL_CLS} flex items-center gap-1.5`}>
            <i className="grid h-4 w-4 place-items-center rounded bg-background text-muted">{ICON[node.kind]}</i>
            {LABEL[node.kind].toLowerCase()}
          </span>
          <b className="mt-1 block truncate font-mono text-[11px] text-foreground">{node.label}</b>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {(isAgentish || node.kind === 'workflow') && (
            <IconButton label="open" onClick={openTarget}><ArrowUpRight size={12} /></IconButton>
          )}
          <IconButton label="close" onClick={onClose}><X size={12} /></IconButton>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-3.5 py-4">
        {agentNode && (
          <>
            <LinkSection
              title="tools" empty="no tools configured yet"
              linked={named(tools.filter((t) => (agentNode.toolIds ?? []).includes(t.id)))}
              available={named(tools.filter((t) => !(agentNode.toolIds ?? []).includes(t.id)))}
              onAdd={(id) => patchAgent(agentNode, 'toolIds', [...(agentNode.toolIds ?? []), id])}
              onRemove={(id) => patchAgent(agentNode, 'toolIds', (agentNode.toolIds ?? []).filter((x) => x !== id))}
            />
            <LinkSection
              title="skills" empty="no skills configured yet"
              linked={named(skills.filter((s) => (agentNode.skillIds ?? []).includes(s.id)))}
              available={named(skills.filter((s) => !(agentNode.skillIds ?? []).includes(s.id)))}
              onAdd={(id) => patchAgent(agentNode, 'skillIds', [...(agentNode.skillIds ?? []), id])}
              onRemove={(id) => patchAgent(agentNode, 'skillIds', (agentNode.skillIds ?? []).filter((x) => x !== id))}
            />
            <LinkSection
              title="integrations" empty="no integrations configured yet"
              linked={named(integrations.filter((i) => (agentNode.integrations ?? []).includes(i.id)))}
              available={named(integrations.filter((i) => !(agentNode.integrations ?? []).includes(i.id)))}
              onAdd={(id) => patchAgent(agentNode, 'integrations', [...(agentNode.integrations ?? []), id])}
              onRemove={(id) => patchAgent(agentNode, 'integrations', (agentNode.integrations ?? []).filter((x) => x !== id))}
            />
          </>
        )}

        {capField && (
          <LinkSection
            title="used by"
            empty={`no agent uses this ${node.kind} yet`}
            linked={namedAgents.filter((a) => (agents.find((x) => x.id === a.id)?.[capField] ?? []).includes(node.id.slice(node.id.indexOf(':') + 1)))}
            available={namedAgents.filter((a) => !(agents.find((x) => x.id === a.id)?.[capField] ?? []).includes(node.id.slice(node.id.indexOf(':') + 1)))}
            onAdd={(agentId) => {
              const a = agents.find((x) => x.id === agentId);
              const itemId = node.id.slice(node.id.indexOf(':') + 1);
              if (a) patchAgent(a, capField, [...(a[capField] ?? []), itemId]);
            }}
            onRemove={(agentId) => {
              const a = agents.find((x) => x.id === agentId);
              const itemId = node.id.slice(node.id.indexOf(':') + 1);
              if (a) patchAgent(a, capField, (a[capField] ?? []).filter((x) => x !== itemId));
            }}
          />
        )}

        {wfNode && (
          <LinkSection
            title="agents in this workflow"
            empty="this workflow has no agent steps yet — add agents on the canvas"
            linked={namedAgents.filter((a) => wfNode.nodes.some((n) => n.agentId === a.id))}
            available={named(agents.filter((a) => !a.isManager && !wfNode.nodes.some((n) => n.agentId === a.id)))}
            onAdd={(agentId) => {
              const a = agents.find((x) => x.id === agentId);
              if (!a) return;
              const nodes = [...wfNode.nodes, { id: `node-${Date.now()}`, type: 'agent' as const, agentId, label: a.name, x: 80 + wfNode.nodes.length * 220, y: 120 }];
              void onSaveWorkflow({ ...wfNode, nodes });
            }}
            onRemove={(agentId) => {
              const nodes = wfNode.nodes.filter((n) => n.agentId !== agentId);
              const kept = new Set(nodes.map((n) => n.id));
              void onSaveWorkflow({ ...wfNode, nodes, edges: wfNode.edges.filter((e) => kept.has(e.from) && kept.has(e.to)) });
            }}
          />
        )}

        {/* A memory node: which scope it is, and what a project scope belongs to. */}
        {node.kind === 'memory' && (
          <section className="mb-5 last:mb-0">
            <div className="mb-2 flex items-baseline justify-between gap-2">
              <span className={GROUP_LABEL_CLS}>scope</span>
            </div>
            <p className="m-0 break-all font-mono text-[11px] text-foreground">{memKey || 'global'}</p>
            <p className="mt-2 mb-0 font-mono text-[10px] leading-relaxed text-muted">{node.sub}</p>
            {memKey.startsWith('project:') && (
              <p className="mt-2 mb-0 font-mono text-[10px] leading-relaxed text-muted">
                {projects.find((p) => p.id === memKey.slice('project:'.length))?.name ? `Project: ${projects.find((p) => p.id === memKey.slice('project:'.length))?.name}` : 'Project no longer exists — this memory is orphaned.'}
              </p>
            )}
          </section>
        )}

        {/* A brain-map node: its kind/scope, and how its edges were formed. */}
        {node.kind === 'brain' && (
          <section className="mb-5 last:mb-0">
            <div className="mb-2 flex items-baseline justify-between gap-2">
              <span className={GROUP_LABEL_CLS}>mind</span>
            </div>
            <p className="m-0 font-mono text-[11px] text-foreground">{node.sub}</p>
            <p className="mt-2 mb-0 font-mono text-[10px] leading-relaxed text-muted">
              Part of the brain map. Dashed relationships were inferred; solid ones were stated. Correct or remove anything wrong from the Memory view.
            </p>
          </section>
        )}

        {node.kind === 'project' && (
          <section className="mb-5 last:mb-0">
            <div className="mb-2 flex items-baseline justify-between gap-2">
              <span className={GROUP_LABEL_CLS}>project</span>
            </div>
            <p className="m-0 font-mono text-[10px] leading-relaxed text-muted">
              Conversations in this project share one memory scope, so everything learned here is recalled together.
            </p>
          </section>
        )}

        {node.kind === 'integration' && (
          <section className="mb-5 last:mb-0">
            <div className="mb-2 flex items-baseline justify-between gap-2">
              <span className={GROUP_LABEL_CLS}>connected</span>
            </div>
            <p className="m-0 font-mono text-[10px] text-muted">
              {integrations.find((i) => `int:${i.id}` === node.id)?.connected ? 'yes' : 'not connected'}
            </p>
          </section>
        )}

        {node.kind === 'tool' && (
          <section className="mb-5 last:mb-0">
            <div className="mb-2 flex items-baseline justify-between gap-2">
              <span className={GROUP_LABEL_CLS}>backed by</span>
            </div>
            <p className="m-0 break-all font-mono text-[10px] text-muted">
              {(() => {
                const t = tools.find((x) => `tool:${x.id}` === node.id);
                if (!t) return 'unknown';
                const integ = integrations.find((i) => i.id === t.integrationId);
                return integ ? integ.name : t.integrationId || 'built in';
              })()}
            </p>
          </section>
        )}
      </div>
    </aside>
  );
}
