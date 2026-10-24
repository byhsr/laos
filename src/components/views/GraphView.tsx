import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Boxes, Maximize2, Network, Orbit, RefreshCw, ZoomIn, ZoomOut } from 'lucide-react';
import type { Agent, BrainEdge, BrainNode, Integration, Project, Skill, Tool, Workflow } from '../../types';
import { IconButton } from '../ui/Button';
import { ICON, Inspector, LABEL, NODE_COLOR, type GEdge, type GNode, type Kind } from './graph/shared';
import { buildGraph, layout, type MemoryScope } from './graph/layout';
import { GROUP_LABEL_CLS } from '../ui/Input';
import { tabCls } from '../ui/tabs';
import { amListScopes, brainGraph } from '../../runtime';

// The attention/salience lens: which signals a brain node can be sized by, and a
// cool→hot colour ramp so a busy node reads at a glance.
const LENS_DIMS = ['allocation', 'dormancy', 'momentum', 'persistence', 'displacement', 'drift', 'intention', 'reentry', 'salience'];
const COOL = [96, 110, 132];
const HOT = [215, 255, 56];
const heat = (v: number) => `rgb(${COOL.map((c, i) => Math.round(c + (HOT[i] - c) * v)).join(', ')})`;

// three.js is a big dependency, so the scene is its own chunk that only loads
// when someone actually opens the 3D view.
const Graph3D = lazy(() => import('./graph/Graph3D'));

const NODE_W = 196;
const NODE_H = 54;
const PAD = 60;
const MIN_ZOOM = 0.2;
const MAX_ZOOM = 2.5;
const FIT_MARGIN = 72;

const clampZoom = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

// The workspace as one spatial graph: every agent, capability, workflow, project
// and memory scope as a node, wired to whatever it is connected to. 3D is the
// default; the flat projection stays one click away.
export function GraphView({ agents, skills, tools, integrations, workflows, projects, onOpenAgent, onOpenWorkflow, onSaveAgent, onSaveWorkflow }: {
  agents: Agent[]; skills: Skill[]; tools: Tool[]; integrations: Integration[]; workflows: Workflow[]; projects: Project[];
  onOpenAgent: (id: string) => void; onOpenWorkflow: (id: string) => void;
  onSaveAgent: (a: Agent) => Promise<void>; onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
}) {
  const canvasRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; y: number; px: number; py: number; moved: boolean } | null>(null);
  const [mode, setMode] = useState<'3d' | '2d'>('3d');
  const [scopes, setScopes] = useState<MemoryScope[]>([]);
  const [brain, setBrain] = useState<{ nodes: BrainNode[]; edges: BrainEdge[] }>({ nodes: [], edges: [] });
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [hover, setHover] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  // Filters + lens. `structure` is the normal view; the other lenses re-size and
  // heat-colour brain nodes by a chosen attention/salience signal.
  const [hiddenKinds, setHiddenKinds] = useState<Set<Kind>>(new Set());
  const [scopeFilter, setScopeFilter] = useState('');
  const [showInferred, setShowInferred] = useState(true);
  const [lens, setLens] = useState<'structure' | 'attention' | 'salience'>('structure');
  const [dim, setDim] = useState('allocation');
  // The scene holds still by default — a drifting view makes reading a node's
  // connections, and clicking it, much harder.
  const [spin, setSpin] = useState(false);

  // Memory + the brain map live in their own store, so they are fetched alongside
  // the rest.
  const loadScopes = useCallback(() => {
    void amListScopes()
      .then((s) => setScopes((s as MemoryScope[]) ?? []))
      .catch(() => setScopes([]));
    void brainGraph()
      .then((g) => setBrain(g ?? { nodes: [], edges: [] }))
      .catch(() => setBrain({ nodes: [], edges: [] }));
  }, []);
  useEffect(() => { loadScopes(); }, [loadScopes]);

  const fullGraph = useMemo(
    () => buildGraph({ agents, skills, tools, integrations, workflows, projects, scopes, brain }),
    [agents, skills, tools, integrations, workflows, projects, scopes, brain],
  );

  // Apply the filters, then the lens. Only brain nodes carry an attention signal,
  // so a lens scales them and leaves the workspace graph as it was.
  const graph = useMemo(() => {
    let nodes = fullGraph.nodes.filter((n) => {
      if (hiddenKinds.has(n.kind)) return false;
      if (n.kind === 'brain') {
        if (scopeFilter && n.scope !== scopeFilter) return false;
        if (!showInferred && n.provenance === 'inferred') return false;
      }
      return true;
    });
    if (lens !== 'structure') {
      const score = (n: GNode) => (lens === 'salience' ? (n.salience ?? 0) : (n.attention?.[dim] ?? 0));
      const max = Math.max(1e-6, ...nodes.filter((n) => n.kind === 'brain').map(score));
      nodes = nodes.map((n) => {
        if (n.kind !== 'brain') return n;
        const v = Math.max(0, Math.min(1, score(n) / max));
        return { ...n, r: 0.34 + v * 1.7, color: heat(v) };
      });
    }
    const keep = new Set(nodes.map((n) => n.id));
    const edges = fullGraph.edges.filter((e) => keep.has(e.from) && keep.has(e.to));
    return { nodes, edges };
  }, [fullGraph, hiddenKinds, scopeFilter, showInferred, lens, dim]);

  const kindsPresent = useMemo(() => Array.from(new Set(fullGraph.nodes.map((n) => n.kind))), [fullGraph.nodes]);
  const brainScopes = useMemo(() => Array.from(new Set(brain.nodes.map((n) => n.scope))).sort(), [brain.nodes]);
  const hasBrain = brain.nodes.length > 0;
  const toggleKind = (k: Kind) => setHiddenKinds((s) => {
    const next = new Set(s);
    if (next.has(k)) next.delete(k); else next.add(k);
    return next;
  });

  // The settled positions, recomputed only when the graph itself changes.
  const positions = useMemo(() => layout(graph.nodes, graph.edges, mode === '3d' ? 3 : 2), [graph, mode]);

  // Flat mode needs the layout in screen terms (0-based, node-sized gutter).
  const flat = useMemo(() => {
    if (mode !== '2d') return null;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    positions.forEach(([x, y]) => {
      minX = Math.min(minX, x); minY = Math.min(minY, y);
      maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
    });
    if (!Number.isFinite(minX)) { minX = 0; minY = 0; maxX = 0; maxY = 0; }
    const at = new Map<string, { x: number; y: number }>();
    graph.nodes.forEach((n) => {
      const p = positions.get(n.id);
      if (p) at.set(n.id, { x: p[0] - minX + PAD + NODE_W / 2, y: p[1] - minY + PAD + NODE_H / 2 });
    });
    return { at, width: (maxX - minX) + NODE_W + PAD * 2, height: (maxY - minY) + NODE_H + PAD * 2 };
  }, [mode, positions, graph.nodes]);

  // A clicked node stays the focus; hover only traces when nothing is selected.
  const focus = selected ?? hover;
  const connected = useMemo(() => {
    if (!focus) return new Set<string>();
    const set = new Set<string>([focus]);
    graph.edges.forEach((e) => { if (e.from === focus) set.add(e.to); if (e.to === focus) set.add(e.from); });
    return set;
  }, [focus, graph.edges]);

  const selNode = selected ? graph.nodes.find((n) => n.id === selected) ?? null : null;
  const pick = (id: string) => setSelected((s) => (id === '' || s === id ? null : id));
  const edgeActive = (e: GEdge) => !!focus && (e.from === focus || e.to === focus);

  // ---- flat-mode pan / zoom -------------------------------------------------
  const zoomRef = useRef(zoom); zoomRef.current = zoom;
  const panRef = useRef(pan); panRef.current = pan;
  const fittedFor = useRef('');

  const fit = useCallback(() => {
    const el = canvasRef.current;
    if (!el || !flat || !el.clientWidth || !el.clientHeight) return;
    const z = clampZoom(Math.min(1, (el.clientWidth - FIT_MARGIN) / flat.width, (el.clientHeight - FIT_MARGIN) / flat.height));
    setZoom(z);
    setPan({ x: (el.clientWidth - flat.width * z) / 2, y: (el.clientHeight - flat.height * z) / 2 });
  }, [flat]);

  // The canvas mounts hidden (0×0) and only gains a size once its tab is shown,
  // so observe it rather than fitting on mount.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el || mode !== '2d' || !flat) return;
    const key = `${flat.width}x${flat.height}`;
    const tryFit = () => {
      if (!el.clientWidth || !el.clientHeight || fittedFor.current === key) return;
      fittedFor.current = key;
      fit();
    };
    const ro = new ResizeObserver(tryFit);
    ro.observe(el);
    tryFit();
    return () => ro.disconnect();
  }, [mode, flat, fit]);

  // Wheel zoom needs a non-passive listener or the browser eats the event.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el || mode !== '2d') return;
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
  }, [mode]);

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

  const counts = (Object.keys(LABEL) as Kind[])
    .map((kind) => ({ kind, n: graph.nodes.filter((node) => node.kind === kind).length }))
    .filter((c) => c.n > 0);

  const overlay = 'absolute z-[6] flex items-center rounded-xl border border-border bg-surface';

  return (
    <div className="flex h-full flex-col">
      {graph.nodes.length === 0 ? (
        <div className="grid flex-1 place-items-center rounded-xl p-8 text-center">
          <div>
            <p className="m-0 font-mono text-[11px] text-foreground">nothing to map yet</p>
            <p className="mx-auto mt-1 max-w-[340px] text-[12px] leading-relaxed text-muted">Add an agent, tool, skill or workflow and it shows up here, wired to whatever it uses.</p>
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 gap-3">
          {/* No surface of its own — the graph floats on the workspace background. */}
          <div
            ref={canvasRef}
            className={`relative min-h-0 flex-1 overflow-hidden ${mode === '2d' ? 'cursor-grab active:cursor-grabbing' : ''}`}
            onPointerDown={mode === '2d' ? (e) => {
              if (e.button !== 0) return;
              if ((e.target as HTMLElement).closest('[data-node]')) return;
              dragRef.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y, moved: false };
              e.currentTarget.setPointerCapture(e.pointerId);
            } : undefined}
            onPointerMove={mode === '2d' ? (e) => {
              const d = dragRef.current;
              if (!d) return;
              if (!d.moved && Math.abs(e.clientX - d.x) + Math.abs(e.clientY - d.y) > 4) d.moved = true;
              if (d.moved) setPan({ x: d.px + (e.clientX - d.x), y: d.py + (e.clientY - d.y) });
            } : undefined}
            onPointerUp={mode === '2d' ? (e) => {
              const d = dragRef.current;
              dragRef.current = null;
              e.currentTarget.releasePointerCapture(e.pointerId);
              if (d && !d.moved) setSelected(null);
            } : undefined}
            onPointerLeave={mode === '2d' ? () => { dragRef.current = null; } : undefined}
          >
            {mode === '3d' ? (
              <Suspense fallback={<div className="grid h-full place-items-center font-mono text-[11px] text-muted">building the scene…</div>}>
                <Graph3D
                  nodes={graph.nodes}
                  edges={graph.edges}
                  positions={positions}
                  focus={focus}
                  connected={connected}
                  selected={selected}
                  spin={spin}
                  onHover={setHover}
                  onSelect={pick}
                />
              </Suspense>
            ) : flat && (
              <div
                className="absolute top-0 left-0 origin-top-left"
                style={{ width: flat.width, height: flat.height, transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}
              >
                <svg className="pointer-events-none absolute top-0 left-0 z-0" width={flat.width} height={flat.height}>
                  <defs>
                    <marker id="gv-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                      <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-border)" />
                    </marker>
                    <marker id="gv-arrow-on" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                      <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-accent)" />
                    </marker>
                  </defs>
                  {graph.edges.map((e) => {
                    const a = flat.at.get(e.from);
                    const b = flat.at.get(e.to);
                    if (!a || !b) return null;
                    const active = edgeActive(e);
                    const dim = !!focus && !active;
                    return (
                      <line
                        key={e.id}
                        x1={a.x} y1={a.y} x2={b.x} y2={b.y}
                        stroke={active ? 'var(--color-accent)' : 'var(--color-border)'}
                        strokeWidth={active ? 1.8 : 1.2}
                        strokeDasharray={e.kind === 'derives' ? '4 3' : undefined}
                        opacity={dim ? 0.15 : 1}
                        markerEnd={active ? 'url(#gv-arrow-on)' : 'url(#gv-arrow)'}
                      />
                    );
                  })}
                </svg>

                {graph.nodes.map((n) => {
                  const p = flat.at.get(n.id);
                  if (!p) return null;
                  const dim = !!focus && !connected.has(n.id);
                  const isSel = selected === n.id;
                  return (
                    <button
                      key={n.id}
                      type="button"
                      data-node
                      onMouseEnter={() => setHover(n.id)}
                      onMouseLeave={() => setHover(null)}
                      onClick={() => setSelected((s) => (s === n.id ? null : n.id))}
                      className={`absolute z-[1] flex cursor-pointer items-center gap-3 rounded-xl border bg-surface px-3 text-left transition-all duration-150 ${dim ? 'opacity-25' : 'opacity-100'} ${isSel ? 'border-accent' : focus === n.id ? 'border-foreground/40' : 'border-border hover:border-foreground/30'}`}
                      style={{ left: p.x - NODE_W / 2, top: p.y - NODE_H / 2, width: NODE_W, height: NODE_H }}
                    >
                      <span className="grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-background" style={{ color: n.color ?? NODE_COLOR[n.kind] }}>{ICON[n.kind]}</span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-mono text-[11px] text-foreground">{n.label}</span>
                        <span className="block truncate font-mono text-[10px] text-muted">{n.sub}</span>
                      </span>
                      <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${isSel ? 'bg-accent' : 'bg-border'}`} />
                    </button>
                  );
                })}
              </div>
            )}

            <div className={`${overlay} top-3 left-3 gap-0.5 p-1`} onPointerDown={(e) => e.stopPropagation()}>
              <IconButton label="3d graph" className={`h-6 w-6 ${mode === '3d' ? 'bg-background text-foreground' : ''}`} onClick={() => setMode('3d')}><Boxes size={13} /></IconButton>
              <IconButton label="2d graph" className={`h-6 w-6 ${mode === '2d' ? 'bg-background text-foreground' : ''}`} onClick={() => setMode('2d')}><Network size={13} /></IconButton>
              <IconButton label="refresh memory" className="h-6 w-6" onClick={loadScopes}><RefreshCw size={13} /></IconButton>
              {mode === '3d' && (
                <IconButton
                  label={spin ? 'stop rotating' : 'rotate slowly'}
                  className={`h-6 w-6 ${spin ? 'bg-background text-foreground' : ''}`}
                  onClick={() => setSpin((s) => !s)}
                >
                  <Orbit size={13} />
                </IconButton>
              )}
            </div>

            {/* Filters + the attention lens: what to show, and what's consuming
                the brain. Only brain nodes carry a signal, so a lens reshapes
                them and leaves the workspace graph alone. */}
            <div className="absolute z-[6] top-14 left-3 w-[252px] rounded-xl border border-border bg-surface p-2.5" onPointerDown={(e) => e.stopPropagation()}>
              <div className="flex items-center gap-1">
                <span className={GROUP_LABEL_CLS}>lens</span>
                <div className="ml-auto flex gap-1">
                  {(['structure', 'attention', 'salience'] as const).map((l) => (
                    <button key={l} className={tabCls(lens === l)} onClick={() => setLens(l)}>{l}</button>
                  ))}
                </div>
              </div>
              {lens === 'attention' && (
                <select value={dim} onChange={(e) => setDim(e.target.value)} className="mt-1.5 h-[26px] w-full rounded-lg border border-border bg-background px-2 font-mono text-[10px] text-foreground outline-none">
                  {LENS_DIMS.map((d) => <option key={d} value={d}>{d}</option>)}
                </select>
              )}
              {lens !== 'structure' && (
                <p className="mt-1.5 mb-0 font-mono text-[9.5px] leading-relaxed text-muted">node size + colour show the signal (cool → hot)</p>
              )}

              <div className="mt-2 flex flex-wrap gap-1">
                {kindsPresent.map((k) => (
                  <button key={k} className={tabCls(!hiddenKinds.has(k))} onClick={() => toggleKind(k)}>{LABEL[k].toLowerCase()}</button>
                ))}
              </div>

              {hasBrain && (
                <div className="mt-2 flex items-center gap-1.5">
                  <select value={scopeFilter} onChange={(e) => setScopeFilter(e.target.value)} className="h-[26px] min-w-0 flex-1 rounded-lg border border-border bg-background px-2 font-mono text-[10px] text-foreground outline-none">
                    <option value="">all scopes</option>
                    {brainScopes.map((s) => <option key={s} value={s}>{s}</option>)}
                  </select>
                  <button className={tabCls(showInferred)} onClick={() => setShowInferred((v) => !v)}>inferred</button>
                </div>
              )}
            </div>

            {mode === '2d' && (
              <div className={`${overlay} top-3 right-3 gap-0.5 p-1`} onPointerDown={(e) => e.stopPropagation()}>
                <IconButton label="zoom out" className="h-6 w-6" onClick={() => zoomBy(0.9)}><ZoomOut size={13} /></IconButton>
                <span className="w-10 text-center font-mono text-[10px] text-muted">{Math.round(zoom * 100)}%</span>
                <IconButton label="zoom in" className="h-6 w-6" onClick={() => zoomBy(1.1)}><ZoomIn size={13} /></IconButton>
                <IconButton label="fit to view" className="h-6 w-6" onClick={fit}><Maximize2 size={13} /></IconButton>
              </div>
            )}

            {mode === '3d' && (
              <div className={`${overlay} pointer-events-none right-3 bottom-3 px-2.5 py-1 font-mono text-[10px] text-muted`}>
                drag to orbit · scroll to zoom
              </div>
            )}

            <div className={`${overlay} pointer-events-none bottom-3 left-3 flex-wrap gap-3 px-3 py-1.5`}>
              {counts.map((c) => (
                <span key={c.kind} className="flex items-center gap-1.5 font-mono text-[10px] text-muted">
                  <i className="grid h-4 w-4 place-items-center rounded bg-background" style={{ color: NODE_COLOR[c.kind] }}>{ICON[c.kind]}</i>
                  {c.n} {LABEL[c.kind].toLowerCase()}
                </span>
              ))}
            </div>
          </div>

          {selNode && (
            <Inspector
              node={selNode}
              agents={agents} skills={skills} tools={tools} integrations={integrations} workflows={workflows} projects={projects}
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
