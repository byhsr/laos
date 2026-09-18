import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { ArrowUpRight, Bot, Globe, Maximize2, Plus, Sparkles, UserCog, Workflow as WorkflowIcon, Wrench, X, ZoomIn, ZoomOut } from 'lucide-react';
import type { Agent, Integration, Skill, Tool, Workflow } from '../../types';

const NODE_W = 196;
const NODE_H = 54;
const COL_GAP = 200;
const ROW_GAP = 14;
const PAD = 40;
const MIN_ZOOM = 0.25;
const MAX_ZOOM = 2;
const FIT_MARGIN = 72;

type Kind = 'skill' | 'tool' | 'integration' | 'agent' | 'manager' | 'workflow';
type GNode = { id: string; kind: Kind; label: string; sub: string; x: number; y: number };
type GEdge = { id: string; from: string; to: string };
type LinkField = 'toolIds' | 'skillIds' | 'integrations';

const ICON: Record<Kind, ReactNode> = {
  skill: <Sparkles size={14} />,
  tool: <Wrench size={14} />,
  integration: <Globe size={14} />,
  agent: <Bot size={14} />,
  manager: <UserCog size={14} />,
  workflow: <WorkflowIcon size={14} />,
};

const LABEL: Record<Kind, string> = {
  skill: 'Skills',
  tool: 'Tools',
  integration: 'Integrations',
  agent: 'Agents',
  manager: 'Manager',
  workflow: 'Workflows',
};

// Which agent field a capability node links through.
const FIELD_OF: Partial<Record<Kind, LinkField>> = {
  tool: 'toolIds',
  skill: 'skillIds',
  integration: 'integrations',
};

const clampZoom = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

function LinkRow({ name, onRemove }: { name: string; onRemove: () => void }) {
  return (
    <div className="group flex items-center gap-2 rounded-md border border-line bg-panel2/50 px-2.5 py-1.5">
      <span className="min-w-0 flex-1 truncate text-[12px] text-text">{name}</span>
      <button
        className="grid h-5 w-5 shrink-0 cursor-pointer place-items-center rounded text-muted opacity-0 transition-opacity hover:bg-[#e11d48] hover:text-white group-hover:opacity-100"
        onClick={onRemove}
        title="Remove link"
      >
        <X size={11} />
      </button>
    </div>
  );
}

function LinkSection({ title, linked, available, onAdd, onRemove, empty }: {
  title: string;
  linked: { id: string; name: string }[];
  available: { id: string; name: string }[];
  onAdd: (id: string) => void;
  onRemove: (id: string) => void;
  empty: string;
}) {
  return (
    <section className="mb-5 last:mb-0">
      <div className="mb-2 flex items-baseline justify-between">
        <span className="font-mono text-[10px] uppercase tracking-[1px] text-muted">{title}</span>
        <span className="font-mono text-[10px] text-muted">{linked.length}</span>
      </div>

      {linked.length === 0 && available.length === 0 ? (
        <p className="text-[11.5px] leading-[1.6] text-muted">{empty}</p>
      ) : (
        <div className="grid gap-1.5">
          {linked.map((i) => <LinkRow key={i.id} name={i.name} onRemove={() => onRemove(i.id)} />)}
          {available.length > 0 && (
            <div className="mt-0.5 flex flex-wrap gap-1.5">
              {available.map((i) => (
                <button
                  key={i.id}
                  className="flex max-w-full cursor-pointer items-center gap-1 rounded-md border border-dashed border-soft px-2 py-1 text-[11px] text-muted transition-colors hover:border-mid hover:text-text"
                  onClick={() => onAdd(i.id)}
                  title={`Link ${i.name}`}
                >
                  <Plus size={10} /><span className="truncate">{i.name}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

function Inspector({ node, agents, skills, tools, integrations, workflows, onClose, onOpenAgent, onOpenWorkflow, onSaveAgent, onSaveWorkflow }: {
  node: GNode;
  agents: Agent[]; skills: Skill[]; tools: Tool[]; integrations: Integration[]; workflows: Workflow[];
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

  return (
    <aside className="glass flex w-[300px] shrink-0 flex-col overflow-hidden rounded-[16px] border border-hairline">
      <header className="flex shrink-0 items-start justify-between gap-2 border-b border-hairline px-3.5 py-3">
        <div className="min-w-0 flex-1">
          <span className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-[1px] text-muted">
            <i className="grid h-4 w-4 place-items-center rounded bg-panel2">{ICON[node.kind]}</i>
            {LABEL[node.kind]}
          </span>
          <b className="mt-1 block truncate text-[13px] text-text">{node.label}</b>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {(isAgentish || node.kind === 'workflow') && (
            <button className="secondary" onClick={openTarget} title="Open"><ArrowUpRight size={12} /></button>
          )}
          <button className="secondary" onClick={onClose} title="Close"><X size={12} /></button>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-3.5 py-4">
        {agentNode && (
          <>
            <LinkSection
              title="Tools" empty="No tools configured yet."
              linked={named(tools.filter((t) => (agentNode.toolIds ?? []).includes(t.id)))}
              available={named(tools.filter((t) => !(agentNode.toolIds ?? []).includes(t.id)))}
              onAdd={(id) => patchAgent(agentNode, 'toolIds', [...(agentNode.toolIds ?? []), id])}
              onRemove={(id) => patchAgent(agentNode, 'toolIds', (agentNode.toolIds ?? []).filter((x) => x !== id))}
            />
            <LinkSection
              title="Skills" empty="No skills configured yet."
              linked={named(skills.filter((s) => (agentNode.skillIds ?? []).includes(s.id)))}
              available={named(skills.filter((s) => !(agentNode.skillIds ?? []).includes(s.id)))}
              onAdd={(id) => patchAgent(agentNode, 'skillIds', [...(agentNode.skillIds ?? []), id])}
              onRemove={(id) => patchAgent(agentNode, 'skillIds', (agentNode.skillIds ?? []).filter((x) => x !== id))}
            />
            <LinkSection
              title="Integrations" empty="No integrations configured yet."
              linked={named(integrations.filter((i) => (agentNode.integrations ?? []).includes(i.id)))}
              available={named(integrations.filter((i) => !(agentNode.integrations ?? []).includes(i.id)))}
              onAdd={(id) => patchAgent(agentNode, 'integrations', [...(agentNode.integrations ?? []), id])}
              onRemove={(id) => patchAgent(agentNode, 'integrations', (agentNode.integrations ?? []).filter((x) => x !== id))}
            />
          </>
        )}

        {capField && (
          <LinkSection
            title="Used by"
            empty={`No agent uses this ${node.kind} yet.`}
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
            title="Agents in this workflow"
            empty="This workflow has no agent steps yet. Add agents on the canvas."
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
      </div>
    </aside>
  );
}

export function GraphView({ agents, skills, tools, integrations, workflows, onOpenAgent, onOpenWorkflow, onSaveAgent, onSaveWorkflow, embedded = false }: {
  agents: Agent[]; skills: Skill[]; tools: Tool[]; integrations: Integration[]; workflows: Workflow[];
  onOpenAgent: (id: string) => void; onOpenWorkflow: (id: string) => void;
  onSaveAgent: (a: Agent) => Promise<void>; onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
  embedded?: boolean;
}) {
  const canvasRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; y: number; px: number; py: number; moved: boolean } | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [hover, setHover] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  // Mirrors so the imperative wheel handler never reads stale state.
  const zoomRef = useRef(zoom); zoomRef.current = zoom;
  const panRef = useRef(pan); panRef.current = pan;
  const fittedFor = useRef('');

  const { nodes, edges, width, height } = useMemo(() => {
    const caps: GNode[] = [];
    skills.forEach((s) => caps.push({ id: `skill:${s.id}`, kind: 'skill', label: s.name, sub: 'instructions', x: PAD, y: PAD + caps.length * (NODE_H + ROW_GAP) }));
    tools.forEach((t) => caps.push({ id: `tool:${t.id}`, kind: 'tool', label: t.name, sub: t.kind.replace('_', ' '), x: PAD, y: PAD + caps.length * (NODE_H + ROW_GAP) }));
    integrations.filter((i) => i.connected).forEach((i) => caps.push({ id: `int:${i.id}`, kind: 'integration', label: i.name, sub: `${i.actions.length} actions`, x: PAD, y: PAD + caps.length * (NODE_H + ROW_GAP) }));

    const col2 = PAD + NODE_W + COL_GAP;
    const agentNodes: GNode[] = agents.map((a, i) => ({
      id: `agent:${a.id}`, kind: a.isManager ? 'manager' : 'agent', label: a.name,
      sub: a.isManager ? 'system agent' : `${(a.toolIds ?? []).length} tools · ${(a.skillIds ?? []).length} skills`,
      x: col2, y: PAD + i * (NODE_H + ROW_GAP),
    }));

    const col3 = col2 + NODE_W + COL_GAP;
    const wfNodes: GNode[] = workflows.map((w, i) => ({
      id: `wf:${w.id}`, kind: 'workflow', label: w.name, sub: `${w.nodes.length} nodes`, x: col3, y: PAD + i * (NODE_H + ROW_GAP),
    }));

    const all = [...caps, ...agentNodes, ...wfNodes];
    const known = new Set(all.map((n) => n.id));
    const list: GEdge[] = [];
    agents.forEach((a) => {
      (a.skillIds ?? []).forEach((sid) => { if (known.has(`skill:${sid}`)) list.push({ id: `s:${sid}->${a.id}`, from: `skill:${sid}`, to: `agent:${a.id}` }); });
      (a.toolIds ?? []).forEach((tid) => { if (known.has(`tool:${tid}`)) list.push({ id: `t:${tid}->${a.id}`, from: `tool:${tid}`, to: `agent:${a.id}` }); });
      (a.integrations ?? []).forEach((iid) => { if (known.has(`int:${iid}`)) list.push({ id: `i:${iid}->${a.id}`, from: `int:${iid}`, to: `agent:${a.id}` }); });
    });
    workflows.forEach((w) => w.nodes.forEach((n) => {
      if (n.agentId && known.has(`agent:${n.agentId}`)) list.push({ id: `w:${w.id}:${n.id}`, from: `agent:${n.agentId}`, to: `wf:${w.id}` });
    }));

    const rows = Math.max(1, caps.length, agentNodes.length, wfNodes.length);
    return {
      nodes: all,
      edges: list,
      height: PAD * 2 + rows * (NODE_H + ROW_GAP),
      width: PAD * 2 + NODE_W * 3 + COL_GAP * 2,
    };
  }, [agents, skills, tools, integrations, workflows]);

  const byId = useMemo(() => new Map(nodes.map((n) => [n.id, n])), [nodes]);
  // A clicked node stays the focus; hover only traces when nothing is selected.
  const focus = selected ?? hover;
  const connected = useMemo(() => {
    if (!focus) return new Set<string>();
    const set = new Set<string>([focus]);
    edges.forEach((e) => { if (e.from === focus) set.add(e.to); if (e.to === focus) set.add(e.from); });
    return set;
  }, [focus, edges]);

  const selNode = selected ? nodes.find((n) => n.id === selected) ?? null : null;

  // Scale the whole graph down until it fits the viewport, then centre it.
  const fit = useCallback(() => {
    const el = canvasRef.current;
    if (!el || !el.clientWidth || !el.clientHeight) return;
    const z = clampZoom(Math.min(1, (el.clientWidth - FIT_MARGIN) / width, (el.clientHeight - FIT_MARGIN) / height));
    setZoom(z);
    setPan({ x: (el.clientWidth - width * z) / 2, y: (el.clientHeight - height * z) / 2 });
  }, [width, height]);

  // The canvas mounts hidden (0×0) and only gains a size once its tab is shown,
  // so observe it rather than fitting on mount. Refit only when content changes.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const key = `${width}x${height}`;
    const ro = new ResizeObserver(() => {
      if (!el.clientWidth || !el.clientHeight) return;
      if (fittedFor.current === key) return;
      fittedFor.current = key;
      fit();
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [fit, width, height]);

  // Wheel zoom needs a non-passive listener or the browser eats the event.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const z = zoomRef.current;
      const next = clampZoom(z * (e.deltaY < 0 ? 1.1 : 0.9));
      const r = el.getBoundingClientRect();
      const mx = e.clientX - r.left;
      const my = e.clientY - r.top;
      const p = panRef.current;
      setZoom(next);
      setPan({ x: mx - (mx - p.x) * (next / z), y: my - (my - p.y) * (next / z) });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [nodes.length]);

  const zoomBy = (factor: number) => {
    const el = canvasRef.current;
    if (!el) return;
    const z = zoomRef.current;
    const next = clampZoom(z * factor);
    const cx = el.clientWidth / 2;
    const cy = el.clientHeight / 2;
    const p = panRef.current;
    setZoom(next);
    setPan({ x: cx - (cx - p.x) * (next / z), y: cy - (cy - p.y) * (next / z) });
  };

  const path = (from: GNode, to: GNode) => {
    const x1 = from.x + NODE_W;
    const y1 = from.y + NODE_H / 2;
    const x2 = to.x;
    const y2 = to.y + NODE_H / 2;
    const dx = Math.max(40, (x2 - x1) / 2);
    return `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`;
  };

  const edgeActive = (e: GEdge) => !!focus && (e.from === focus || e.to === focus);
  const nodeDim = (id: string) => !!focus && !connected.has(id);

  const counts = [
    { kind: 'agent' as Kind, n: agents.filter((a) => !a.isManager).length },
    { kind: 'manager' as Kind, n: agents.filter((a) => a.isManager).length },
    { kind: 'skill' as Kind, n: skills.length },
    { kind: 'tool' as Kind, n: tools.length },
    { kind: 'integration' as Kind, n: integrations.filter((i) => i.connected).length },
    { kind: 'workflow' as Kind, n: workflows.length },
  ];

  const zoomBtn = 'grid h-6 w-6 cursor-pointer place-items-center rounded-md border-0 bg-transparent text-muted transition-colors hover:bg-white/5 hover:text-text';

  return (
    <div className="flex h-full min-h-[420px] flex-col">
      {!embedded && (
        <header className="mb-4 flex items-end justify-between gap-4">
          <div className="min-w-0">
            <span className="font-mono text-[11px] tracking-[1px] text-muted">WORKSPACE</span>
            <h1 className="m-0 text-[24px]">Graph</h1>
            <p className="mt-1 text-[12px] leading-[1.6] text-muted">What is wired to what. Drag to pan, scroll to zoom, click a node to see and edit its links.</p>
          </div>
        </header>
      )}

      {nodes.length === 0 ? (
        <div className="grid flex-1 place-items-center rounded-[16px] p-8 text-center">
          <div>
            <p className="text-[13px] text-text">Nothing to map yet</p>
            <p className="mx-auto mt-1 max-w-[340px] text-[12px] leading-[1.6] text-muted">Add an agent, tool, skill or workflow and it shows up here, wired to whatever it uses.</p>
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 gap-3">
          {/* No surface of its own — the graph floats on the workspace background. */}
          <div
            ref={canvasRef}
            className="relative min-h-0 flex-1 cursor-grab overflow-hidden active:cursor-grabbing"
            onPointerDown={(e) => {
              if (e.button !== 0) return;
              // Nodes handle their own presses — capturing here would swallow their click.
              if ((e.target as HTMLElement).closest('[data-node]')) return;
              dragRef.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y, moved: false };
              e.currentTarget.setPointerCapture(e.pointerId);
            }}
            onPointerMove={(e) => {
              const d = dragRef.current;
              if (!d) return;
              if (!d.moved && Math.abs(e.clientX - d.x) + Math.abs(e.clientY - d.y) > 4) d.moved = true;
              if (d.moved) setPan({ x: d.px + (e.clientX - d.x), y: d.py + (e.clientY - d.y) });
            }}
            onPointerUp={(e) => {
              const d = dragRef.current;
              dragRef.current = null;
              e.currentTarget.releasePointerCapture(e.pointerId);
              if (d && !d.moved) setSelected(null);
            }}
            onPointerLeave={() => { dragRef.current = null; }}
          >
            <div
              className="absolute top-0 left-0 origin-top-left"
              style={{ width, height, transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}
            >
              <svg className="pointer-events-none absolute top-0 left-0 z-0" width={width} height={height}>
                <defs>
                  <marker id="gv-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-soft)" />
                  </marker>
                  <marker id="gv-arrow-on" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-green)" />
                  </marker>
                </defs>
                {edges.map((e) => {
                  const from = byId.get(e.from);
                  const to = byId.get(e.to);
                  if (!from || !to) return null;
                  const active = edgeActive(e);
                  return (
                    <g key={e.id}>
                      <path
                        d={path(from, to)}
                        className={`fill-none ${active ? 'stroke-[var(--green)]' : 'stroke-soft'}`}
                        strokeWidth={active ? 1.8 : 1.2}
                        markerEnd={active ? 'url(#gv-arrow-on)' : 'url(#gv-arrow)'}
                      />
                      {active && (
                        <>
                          <circle cx={from.x + NODE_W} cy={from.y + NODE_H / 2} r={3} fill="var(--color-green)" />
                          <circle cx={to.x} cy={to.y + NODE_H / 2} r={3} fill="var(--color-green)" />
                        </>
                      )}
                    </g>
                  );
                })}
              </svg>

              {nodes.map((n) => {
                const dim = nodeDim(n.id);
                const isSel = selected === n.id;
                return (
                  <button
                    key={n.id}
                    type="button"
                    data-node
                    onMouseEnter={() => setHover(n.id)}
                    onMouseLeave={() => setHover(null)}
                    onClick={() => setSelected((s) => (s === n.id ? null : n.id))}
                    className={`absolute z-[1] flex cursor-pointer items-center gap-3 rounded-[14px] border bg-panel px-3 text-left shadow-soft transition-all duration-150 ${dim ? 'opacity-25' : 'opacity-100'} ${isSel ? 'border-[var(--green)]' : focus === n.id ? 'border-mid' : 'border-line hover:border-mid'}`}
                    style={{ left: n.x, top: n.y, width: NODE_W, height: NODE_H }}
                  >
                    <span className="grid h-7 w-7 shrink-0 place-items-center rounded-full bg-panel2 text-muted">{ICON[n.kind]}</span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[12px] text-text">{n.label}</span>
                      <span className="block truncate font-mono text-[10.5px] text-muted">{n.sub}</span>
                    </span>
                    <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${isSel ? 'bg-[var(--green)]' : 'bg-soft'}`} />
                  </button>
                );
              })}
            </div>

            <div
              className="glass absolute top-3 right-3 z-[6] flex items-center gap-0.5 rounded-lg border border-hairline p-1"
              onPointerDown={(e) => e.stopPropagation()}
            >
              <button className={zoomBtn} title="Zoom out" onClick={() => zoomBy(0.9)}><ZoomOut size={13} /></button>
              <span className="w-10 text-center font-mono text-[11px] text-muted">{Math.round(zoom * 100)}%</span>
              <button className={zoomBtn} title="Zoom in" onClick={() => zoomBy(1.1)}><ZoomIn size={13} /></button>
              <button className={zoomBtn} title="Fit to view" onClick={fit}><Maximize2 size={13} /></button>
            </div>

            <div className="glass pointer-events-none absolute bottom-3 left-3 z-[6] flex flex-wrap items-center gap-3 rounded-lg border border-hairline px-3 py-1.5">
              {counts.filter((c) => c.n > 0).map((c) => (
                <span key={c.kind} className="flex items-center gap-1.5 font-mono text-[10.5px] text-muted">
                  <i className="grid h-4 w-4 place-items-center rounded bg-panel2">{ICON[c.kind]}</i>
                  {c.n} {LABEL[c.kind].toLowerCase()}
                </span>
              ))}
            </div>
          </div>

          {selNode && (
            <Inspector
              node={selNode}
              agents={agents} skills={skills} tools={tools} integrations={integrations} workflows={workflows}
              onClose={() => setSelected(null)}
              onOpenAgent={onOpenAgent}
              onOpenWorkflow={onOpenWorkflow}
              onSaveAgent={onSaveAgent}
              onSaveWorkflow={onSaveWorkflow}
            />
          )}
        </div>
      )}
    </div>
  );
}
