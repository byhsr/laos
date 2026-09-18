import { useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { Bot, Globe, Maximize2, Sparkles, UserCog, Workflow as WorkflowIcon, Wrench, ZoomIn, ZoomOut } from 'lucide-react';
import type { Agent, Integration, Skill, Tool, Workflow } from '../../types';

const NODE_W = 196;
const NODE_H = 54;
const COL_GAP = 200;
const ROW_GAP = 14;
const PAD = 40;

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

export function GraphView({ agents, skills, tools, integrations, workflows, onOpenAgent, onOpenWorkflow, embedded = false }: {
  agents: Agent[]; skills: Skill[]; tools: Tool[]; integrations: Integration[]; workflows: Workflow[];
  onOpenAgent: (id: string) => void; onOpenWorkflow: (id: string) => void; embedded?: boolean;
}) {
  const canvasRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [hover, setHover] = useState<string | null>(null);

  const { nodes, edges, width, height } = useMemo(() => {
    const caps: GNode[] = [];
    skills.forEach((s) => caps.push({ id: `skill:${s.id}`, kind: 'skill', label: s.name, sub: 'instructions', x: PAD, y: PAD + caps.length * (NODE_H + ROW_GAP) }));
    tools.forEach((t) => caps.push({ id: `tool:${t.id}`, kind: 'tool', label: t.name, sub: t.kind.replace('_', ' '), x: PAD, y: PAD + caps.length * (NODE_H + ROW_GAP) }));
    integrations.filter((i) => i.connected).forEach((i) => caps.push({ id: `int:${i.id}`, kind: 'integration', label: i.name, sub: `${i.actions.length} actions`, x: PAD, y: PAD + caps.length * (NODE_H + ROW_GAP) }));

    const col2 = PAD + NODE_W + COL_GAP;
    const agentNodes: GNode[] = agents.map((a, i) => ({
      id: `agent:${a.id}`, kind: a.isManager ? 'manager' : 'agent', label: a.name,
      sub: a.isManager ? 'system agent' : `${a.toolIds.length} tools · ${(a.skillIds ?? []).length} skills`,
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
      a.toolIds.forEach((tid) => { if (known.has(`tool:${tid}`)) list.push({ id: `t:${tid}->${a.id}`, from: `tool:${tid}`, to: `agent:${a.id}` }); });
      a.integrations.forEach((iid) => { if (known.has(`int:${iid}`)) list.push({ id: `i:${iid}->${a.id}`, from: `int:${iid}`, to: `agent:${a.id}` }); });
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

  // Wheel zoom needs a non-passive listener or the browser eats the event.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      setZoom((z) => Math.min(2, Math.max(0.35, z * (e.deltaY < 0 ? 1.1 : 0.9))));
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

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

  const zoomBtn = 'grid h-7 w-7 cursor-pointer place-items-center rounded-[10px] border border-line bg-panel2 text-muted transition-colors hover:text-text';

  return (
    <div className="flex h-[calc(100vh-120px)] min-h-[440px] flex-col">
      <header className="mb-4 flex items-end justify-between gap-4">
        {!embedded && (
          <div className="min-w-0">
            <span className="font-mono text-[11px] tracking-[1px] text-muted">WORKSPACE</span>
            <h1 className="m-0 text-[24px]">Graph</h1>
            <p className="mt-1 text-[12px] leading-[1.6] text-muted">What is wired to what — skills, tools and integrations feeding agents, and the workflows those agents run in. Drag to pan, scroll to zoom, hover to trace.</p>
          </div>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          <button className={zoomBtn} title="Zoom out" onClick={() => setZoom((z) => Math.max(0.35, z * 0.9))}><ZoomOut size={13} /></button>
          <span className="w-9 text-center font-mono text-[11px] text-muted">{Math.round(zoom * 100)}%</span>
          <button className={zoomBtn} title="Zoom in" onClick={() => setZoom((z) => Math.min(2, z * 1.1))}><ZoomIn size={13} /></button>
          <button className={zoomBtn} title="Reset view" onClick={() => { setZoom(1); setPan({ x: 0, y: 0 }); }}><Maximize2 size={13} /></button>
        </div>
      </header>

      {nodes.length === 0 ? (
        <div className="grid flex-1 place-items-center rounded-[16px] border border-line bg-panel">
          <p className="text-[12px] text-muted">Nothing to map yet. Add an agent, tool, skill or workflow.</p>
        </div>
      ) : (
        <div
          ref={canvasRef}
          className="relative min-h-0 flex-1 cursor-grab overflow-hidden rounded-[16px] border border-line bg-panel active:cursor-grabbing"
          onPointerDown={(e) => {
            if (e.button !== 0) return;
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

          <div className="pointer-events-none absolute bottom-3 left-3 flex flex-wrap items-center gap-3 rounded-[12px] border border-line bg-panel/90 px-3 py-2 backdrop-blur">
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
