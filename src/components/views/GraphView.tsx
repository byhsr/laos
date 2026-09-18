import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { Bot, Globe, Maximize2, Sparkles, UserCog, Workflow as WorkflowIcon, Wrench, ZoomIn, ZoomOut } from 'lucide-react';
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

const clampZoom = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

export function GraphView({ agents, skills, tools, integrations, workflows, onOpenAgent, onOpenWorkflow, embedded = false }: {
  agents: Agent[]; skills: Skill[]; tools: Tool[]; integrations: Integration[]; workflows: Workflow[];
  onOpenAgent: (id: string) => void; onOpenWorkflow: (id: string) => void; embedded?: boolean;
}) {
  const canvasRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [hover, setHover] = useState<string | null>(null);

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
  const connected = useMemo(() => {
    if (!hover) return new Set<string>();
    const set = new Set<string>([hover]);
    edges.forEach((e) => { if (e.from === hover) set.add(e.to); if (e.to === hover) set.add(e.from); });
    return set;
  }, [hover, edges]);

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

  // Zoom toward the centre of the viewport for the button controls.
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

  const edgeActive = (e: GEdge) => !!hover && (e.from === hover || e.to === hover);
  const nodeDim = (id: string) => !!hover && !connected.has(id);

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
            <p className="mt-1 text-[12px] leading-[1.6] text-muted">What is wired to what — skills, tools and integrations feeding agents, and the workflows those agents run in. Drag to pan, scroll to zoom, hover to trace.</p>
          </div>
        </header>
      )}

      {nodes.length === 0 ? (
        <div className="grid flex-1 place-items-center rounded-[16px] border border-dashed border-soft p-8 text-center">
          <div>
            <p className="text-[13px] text-text">Nothing to map yet</p>
            <p className="mx-auto mt-1 max-w-[340px] text-[12px] leading-[1.6] text-muted">Add an agent, tool, skill or workflow and it shows up here, wired to whatever it uses.</p>
          </div>
        </div>
      ) : (
        <div
          ref={canvasRef}
          className="relative min-h-0 flex-1 cursor-grab overflow-hidden rounded-[16px] border border-line bg-panel active:cursor-grabbing"
          onPointerDown={(e) => {
            if (e.button !== 0) return;
            // Nodes handle their own presses — starting a pan here would capture
            // the pointer and swallow the node's click.
            if ((e.target as HTMLElement).closest('[data-node]')) return;
            dragRef.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
            e.currentTarget.setPointerCapture(e.pointerId);
          }}
          onPointerMove={(e) => {
            const d = dragRef.current;
            if (!d) return;
            setPan({ x: d.px + (e.clientX - d.x), y: d.py + (e.clientY - d.y) });
          }}
          onPointerUp={(e) => { dragRef.current = null; e.currentTarget.releasePointerCapture(e.pointerId); }}
          onPointerLeave={() => { dragRef.current = null; }}
        >
          <div
            className="absolute top-0 left-0 origin-top-left"
            style={{ width, height, transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}
          >
            <svg className="pointer-events-none absolute top-0 left-0 z-0" width={width} height={height}>
              {edges.map((e) => {
                const from = byId.get(e.from);
                const to = byId.get(e.to);
                if (!from || !to) return null;
                const active = edgeActive(e);
                return (
                  <path
                    key={e.id}
                    d={path(from, to)}
                    className={`fill-none ${active ? 'stroke-[var(--green)]' : 'stroke-line'}`}
                    strokeWidth={active ? 1.8 : 1.2}
                  />
                );
              })}
            </svg>

            {nodes.map((n) => {
              const dim = nodeDim(n.id);
              return (
                <button
                  key={n.id}
                  type="button"
                  data-node
                  onMouseEnter={() => setHover(n.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={() => {
                    if (n.kind === 'agent' || n.kind === 'manager') onOpenAgent(n.id.slice('agent:'.length));
                    if (n.kind === 'workflow') onOpenWorkflow(n.id.slice('wf:'.length));
                  }}
                  className={`absolute z-[1] flex cursor-pointer items-center gap-3 rounded-[16px] border bg-panel px-3 text-left transition-all duration-150 ${dim ? 'opacity-25' : 'opacity-100'} ${hover === n.id ? 'border-mid' : 'border-line hover:border-mid'}`}
                  style={{ left: n.x, top: n.y, width: NODE_W, height: NODE_H }}
                >
                  <span className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-panel2 text-muted">{ICON[n.kind]}</span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[12px] text-text">{n.label}</span>
                    <span className="block truncate font-mono text-[10.5px] text-muted">{n.sub}</span>
                  </span>
                </button>
              );
            })}
          </div>

          {/* Zoom controls float over the canvas so the surface stays uncluttered */}
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
      )}
    </div>
  );
}
