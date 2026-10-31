import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react';
import { Orbit, RefreshCw, SlidersHorizontal } from 'lucide-react';
import type { Agent, BrainEdge, BrainNode, Integration, Project, Skill, Tool, Workflow } from '../../types';
import { IconButton } from '../ui/Button';
import { Select } from '../ui/Select';
import { ICON, Inspector, LABEL, NODE_COLOR, type GNode, type Kind } from './graph/shared';
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
// when someone actually opens the graph view.
const Graph3D = lazy(() => import('./graph/Graph3D'));

// The workspace as one spatial graph: every agent, capability, workflow, project
// and memory scope as a node, wired to whatever it is connected to. It is 3D
// only. Filters and the node inspector live in the right-hand tabbed panel.
export function GraphView({ agents, skills, tools, integrations, workflows, projects, onOpenAgent, onOpenWorkflow, onSaveAgent, onSaveWorkflow }: {
  agents: Agent[]; skills: Skill[]; tools: Tool[]; integrations: Integration[]; workflows: Workflow[]; projects: Project[];
  onOpenAgent: (id: string) => void; onOpenWorkflow: (id: string) => void;
  onSaveAgent: (a: Agent) => Promise<void>; onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
}) {
  const [scopes, setScopes] = useState<MemoryScope[]>([]);
  const [brain, setBrain] = useState<{ nodes: BrainNode[]; edges: BrainEdge[] }>({ nodes: [], edges: [] });
  const [hover, setHover] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  // Filters + lens. `structure` is the normal view; the other lenses re-size and
  // heat-colour brain nodes by a chosen attention/salience signal.
  const [hiddenKinds, setHiddenKinds] = useState<Set<Kind>>(new Set());
  const [scopeFilter, setScopeFilter] = useState('');
  const [showInferred, setShowInferred] = useState(true);
  const [lens, setLens] = useState<'structure' | 'attention' | 'salience'>('structure');
  const [dim, setDim] = useState('allocation');
  // The right panel starts closed (a mini button toggles it); inside, filters is
  // the default tab and the inspector opens when a node is picked.
  const [panelOpen, setPanelOpen] = useState(false);
  const [rightTab, setRightTab] = useState<'filters' | 'inspect'>('filters');
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
      // Scope narrows both the brain map and the memory globs — a memory node
      // carries its scope key, so one select covers both.
      if ((n.kind === 'brain' || n.kind === 'memory') && scopeFilter && n.scope !== scopeFilter) return false;
      if (n.kind === 'brain' && !showInferred && n.provenance === 'inferred') return false;
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
  const kindCounts = useMemo(() => {
    const m = new Map<Kind, number>();
    fullGraph.nodes.forEach((n) => m.set(n.kind, (m.get(n.kind) ?? 0) + 1));
    return m;
  }, [fullGraph.nodes]);
  // Every scope that holds something: brain-map scopes plus the loaded memory
  // scopes (which include agent and project scopes the brain map has no node for).
  const scopeOptions = useMemo(() => {
    const keys = new Set<string>();
    brain.nodes.forEach((n) => { if (n.scope) keys.add(n.scope); });
    scopes.forEach((s) => { if (s.namespace.key) keys.add(s.namespace.key); });
    return Array.from(keys).sort();
  }, [brain.nodes, scopes]);
  const hasBrain = brain.nodes.length > 0;

  const toggleKind = (k: Kind) => setHiddenKinds((s) => {
    const next = new Set(s);
    if (next.has(k)) next.delete(k); else next.add(k);
    return next;
  });
  const resetFilters = () => {
    setHiddenKinds(new Set());
    setScopeFilter('');
    setShowInferred(true);
    setLens('structure');
  };

  // Positions come from the FULL graph, so toggling a filter only hides nodes —
  // it no longer re-runs the force layout and makes the whole graph jump.
  const positions = useMemo(() => layout(fullGraph.nodes, fullGraph.edges, 3), [fullGraph]);

  // A clicked node stays the focus; hover only traces when nothing is selected.
  const focus = selected ?? hover;
  const connected = useMemo(() => {
    if (!focus) return new Set<string>();
    const set = new Set<string>([focus]);
    graph.edges.forEach((e) => { if (e.from === focus) set.add(e.to); if (e.to === focus) set.add(e.from); });
    return set;
  }, [focus, graph.edges]);

  const selNode = selected ? graph.nodes.find((n) => n.id === selected) ?? null : null;
  const pick = (id: string) => {
    const next = id === '' || id === selected ? null : id;
    setSelected(next);
    // Picking a node reveals the panel on the inspector; clearing returns the
    // panel to filters but leaves it however the user left it.
    if (next) { setRightTab('inspect'); setPanelOpen(true); } else { setRightTab('filters'); }
  };

  const counts = (Object.keys(LABEL) as Kind[])
    .map((kind) => ({ kind, n: graph.nodes.filter((node) => node.kind === kind).length }))
    .filter((c) => c.n > 0);

  const overlay = 'absolute z-[6] flex items-center rounded-xl border border-border bg-surface';

  return (
    <div className="flex h-full flex-col">
      {fullGraph.nodes.length === 0 ? (
        <div className="grid flex-1 place-items-center rounded-xl p-8 text-center">
          <div>
            <p className="m-0 font-mono text-[11px] text-foreground">nothing to map yet</p>
            <p className="mx-auto mt-1 max-w-[340px] text-[12px] leading-relaxed text-muted">Add an agent, tool, skill or workflow and it shows up here, wired to whatever it uses.</p>
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 gap-3">
          {/* No surface of its own — the graph floats on the workspace background. */}
          <div className="relative min-h-0 flex-1 overflow-hidden">
            {graph.nodes.length === 0 ? (
              <div className="grid h-full place-items-center font-mono text-[11px] text-muted">no nodes match the filters</div>
            ) : (
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
            )}

            <div className={`${overlay} top-3 left-3 gap-0.5 p-1`}>
              <IconButton label="refresh memory" className="h-6 w-6" onClick={loadScopes}><RefreshCw size={13} /></IconButton>
              <IconButton
                label={spin ? 'stop rotating' : 'rotate slowly'}
                className={`h-6 w-6 ${spin ? 'bg-background text-foreground' : ''}`}
                onClick={() => setSpin((s) => !s)}
              >
                <Orbit size={13} />
              </IconButton>
            </div>

            {/* The mini button that opens the filters/inspector panel. */}
            <div className={`${overlay} top-3 right-3 gap-0.5 p-1`}>
              <IconButton
                label={panelOpen ? 'hide filters and inspector' : 'show filters and inspector'}
                className={`h-6 w-6 ${panelOpen ? 'bg-background text-foreground' : ''}`}
                onClick={() => setPanelOpen((v) => !v)}
              >
                <SlidersHorizontal size={13} />
              </IconButton>
            </div>

            <div className={`${overlay} pointer-events-none right-3 bottom-3 px-2.5 py-1 font-mono text-[10px] text-muted`}>
              drag to orbit · scroll to zoom
            </div>

            <div className={`${overlay} pointer-events-none bottom-3 left-3 flex-wrap gap-3 px-3 py-1.5`}>
              {counts.map((c) => (
                <span key={c.kind} className="flex items-center gap-1.5 font-mono text-[10px] text-muted">
                  <i className="grid h-4 w-4 place-items-center rounded bg-background" style={{ color: NODE_COLOR[c.kind] }}>{ICON[c.kind]}</i>
                  {c.n} {LABEL[c.kind].toLowerCase()}
                </span>
              ))}
            </div>
          </div>

          {/* Right panel: filters + the node inspector, one column, two tabs.
              Closed by default — the mini button in the top-right toggles it. */}
          {panelOpen && (
          <aside className="flex w-[300px] shrink-0 flex-col overflow-hidden rounded-xl border border-border bg-surface">
            <header className="flex shrink-0 items-center gap-1 border-b border-border px-3 py-2">
              <button className={tabCls(rightTab === 'filters')} onClick={() => setRightTab('filters')}>filters</button>
              <button className={tabCls(rightTab === 'inspect')} onClick={() => setRightTab('inspect')}>
                inspect{selNode ? <span className="h-1.5 w-1.5 rounded-full bg-accent" /> : null}
              </button>
            </header>

            {rightTab === 'filters' ? (
              <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto p-3">
                <div className="flex items-center gap-1">
                  <span className={GROUP_LABEL_CLS}>lens</span>
                  <div className="ml-auto flex gap-1">
                    {(['structure', 'attention', 'salience'] as const).map((l) => (
                      <button key={l} className={tabCls(lens === l)} onClick={() => setLens(l)}>{l}</button>
                    ))}
                  </div>
                </div>
                {lens === 'attention' && (
                  <div className="mt-1.5">
                    <Select value={dim} options={LENS_DIMS.map((d) => ({ value: d, label: d }))} onChange={setDim} placeholder="signal…" />
                  </div>
                )}
                {lens !== 'structure' && (
                  <p className="mt-1.5 mb-0 font-mono text-[9.5px] leading-relaxed text-muted">node size + colour show the signal (cool → hot)</p>
                )}

                <div className="mt-3 flex items-center gap-1">
                  <span className={GROUP_LABEL_CLS}>node types</span>
                  <button className={`${tabCls(false)} ml-auto`} onClick={resetFilters}>reset</button>
                </div>
                <div className="mt-1.5 flex flex-wrap gap-1">
                  {kindsPresent.map((k) => (
                    <button key={k} className={tabCls(!hiddenKinds.has(k))} onClick={() => toggleKind(k)}>
                      {LABEL[k].toLowerCase()}
                      <span className="font-mono text-[9px] opacity-70">{kindCounts.get(k) ?? 0}</span>
                    </button>
                  ))}
                </div>

                {scopeOptions.length > 0 && (
                  <div className="mt-3">
                    <span className={GROUP_LABEL_CLS}>scope</span>
                    <div className="mt-1.5">
                      <Select
                        value={scopeFilter}
                        options={[{ value: '', label: 'all scopes' }, ...scopeOptions.map((s) => ({ value: s, label: s }))]}
                        onChange={setScopeFilter}
                        placeholder="all scopes"
                      />
                    </div>
                    {hasBrain && (
                      <button className={`${tabCls(showInferred)} mt-1.5`} onClick={() => setShowInferred((v) => !v)}>inferred relationships</button>
                    )}
                  </div>
                )}

                <p className="mt-3 mb-0 font-mono text-[10px] leading-relaxed text-muted">
                  {graph.nodes.length} of {fullGraph.nodes.length} nodes shown
                </p>
              </div>
            ) : selNode ? (
              <Inspector
                node={selNode}
                agents={agents} skills={skills} tools={tools} integrations={integrations} workflows={workflows} projects={projects}
                onClose={() => pick(selNode.id)}
                onOpenAgent={onOpenAgent}
                onOpenWorkflow={onOpenWorkflow}
                onSaveAgent={onSaveAgent}
                onSaveWorkflow={onSaveWorkflow}
              />
            ) : (
              <p className="m-0 p-4 font-mono text-[11px] text-muted">select a node to inspect it</p>
            )}
          </aside>
          )}
        </div>
      )}
    </div>
  );
}
