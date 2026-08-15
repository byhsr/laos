import { useRef, useState } from 'react';
import { Bot, Check, GitBranch, Play, Plus, Repeat, Save, ShieldCheck, Trash2, Webhook, Workflow as WorkflowIcon, X } from 'lucide-react';
import type { Agent, Tool, Workflow, WorkflowEdge, WorkflowNode, WorkflowNodeType, WorkflowRunResult } from '../../types';
import { Dropdown } from '../ui/Dropdown';
import { toast } from '../../hooks/useToast';

const NODE_TYPES: { type: WorkflowNodeType; label: string; icon: React.ReactNode; desc: string; color: string }[] = [
  { type: 'trigger', label: 'Trigger', icon: <Webhook size={13} />, desc: 'Workflow entry point', color: '#22c55e' },
  { type: 'agent', label: 'Agent', icon: <Bot size={13} />, desc: 'Run an agent', color: '#38bdf8' },
  { type: 'subagent', label: 'Subagent', icon: <GitBranch size={13} />, desc: 'Delegate to a sub-agent', color: '#a78bfa' },
  { type: 'loop', label: 'Loop', icon: <Repeat size={13} />, desc: 'Repeat until done', color: '#facc15' },
  { type: 'checker', label: 'Checker', icon: <ShieldCheck size={13} />, desc: 'Validate output', color: '#f87171' },
  { type: 'integration', label: 'Integration', icon: <Webhook size={13} />, desc: 'Call a tool/integration', color: '#34d399' },
  { type: 'gate', label: 'Gate', icon: <GitBranch size={13} />, desc: 'Conditional branch', color: '#fb923c' },
];

const TYPE_META: Record<WorkflowNodeType, { label: string; color: string; icon: React.ReactNode }> = Object.fromEntries(
  NODE_TYPES.map((n) => [n.type, { label: n.label, color: n.color, icon: n.icon }])
) as Record<WorkflowNodeType, { label: string; color: string; icon: React.ReactNode }>;

const NODE_W = 200;
const NODE_H = 84;

export function CanvasView({ agents, tools, workflows, onSaveWorkflow, onDeleteWorkflow, onRunWorkflow }: {
  agents: Agent[]; tools: Tool[]; workflows: Workflow[];
  onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
  onDeleteWorkflow: (id: string) => Promise<void>;
  onRunWorkflow: (w: Workflow, input: string) => Promise<WorkflowRunResult>;
}) {
  const [current, setCurrent] = useState<Workflow | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [draft, setDraft] = useState<WorkflowNode | null>(null);
  const [runInput, setRunInput] = useState('');
  const [running, setRunning] = useState(false);
  const [runResult, setRunResult] = useState<WorkflowRunResult | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [drag, setDrag] = useState<{ type: WorkflowNodeType } | null>(null);
  const [dragPos, setDragPos] = useState<{ x: number; y: number } | null>(null);
  const [moving, setMoving] = useState<{ id: string; dx: number; dy: number } | null>(null);
  const [drawingEdge, setDrawingEdge] = useState<{ from: string; x: number; y: number } | null>(null);
  const canvasRef = useRef<HTMLDivElement>(null);

  const update = (fn: (w: Workflow) => Workflow) => {
    setCurrent((prev) => prev ? fn(prev) : prev);
  };

  const addNode = (type: WorkflowNodeType, x: number, y: number) => {
    const meta = TYPE_META[type];
    const id = `n-${Date.now()}`;
    const node: WorkflowNode = { id, type, label: meta.label, x, y, config: {} };
    update((w) => ({ ...w, nodes: [...w.nodes, node] }));
    setSelectedNodeId(id);
  };

  const moveNode = (id: string, x: number, y: number) => {
    update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === id ? { ...n, x, y } : n)) }));
  };

  const deleteNode = (id: string) => {
    update((w) => ({ ...w, nodes: w.nodes.filter((n) => n.id !== id), edges: w.edges.filter((e) => e.from !== id && e.to !== id) }));
    setSelectedNodeId((s) => (s === id ? null : s));
  };

  const agentName = (id?: string) => agents.find((a) => a.id === id)?.name ?? 'Select agent…';

  const save = async () => {
    if (!current) return;
    setSaving(true);
    try {
      const saved = await onSaveWorkflow(current);
      setCurrent(saved);
      toast(`Workflow "${saved.name}" saved`, 'success');
    } finally {
      setSaving(false);
    }
  };

  const run = async () => {
    if (!current) return;
    setRunning(true); setRunError(null); setRunResult(null);
    try {
      const result = await onRunWorkflow(current, runInput);
      setRunResult(result);
      toast(`Workflow run complete — ${(result.totalPromptTokens + result.totalCompletionTokens).toLocaleString()} tokens`, 'success');
    } catch (e) {
      setRunError(e instanceof Error ? e.message : String(e));
      toast(e instanceof Error ? e.message : 'Workflow run failed', 'error');
    } finally {
      setRunning(false);
    }
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    if (drag) setDragPos({ x, y });
    if (moving) {
      moveNode(moving.id, Math.round(x - moving.dx), Math.round(y - moving.dy));
    }
    if (drawingEdge) setDrawingEdge({ ...drawingEdge, x, y });
  };

  const onPointerUp = () => {
    if (drag && dragPos) { addNode(drag.type, dragPos.x - NODE_W / 2, dragPos.y - 20); }
    setDrag(null); setDragPos(null); setMoving(null); setDrawingEdge(null);
  };

  const startPaletteDrag = (type: WorkflowNodeType) => (e: React.PointerEvent) => {
    e.preventDefault();
    setDrag({ type });
    setDragPos(null);
    // Track the latest pointer position; attach listeners to window so the drag works anywhere.
    let lastX = e.clientX, lastY = e.clientY;
    const onMove = (ev: PointerEvent) => {
      lastX = ev.clientX; lastY = ev.clientY;
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return;
      setDragPos({ x: ev.clientX - rect.left, y: ev.clientY - rect.top });
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      const rect = canvasRef.current?.getBoundingClientRect();
      const over = rect && lastX >= rect.left && lastX <= rect.right && lastY >= rect.top && lastY <= rect.bottom;
      setDrag((d) => {
        if (d && over) {
          setDragPos((pos) => {
            if (pos) addNode(d.type, pos.x - NODE_W / 2, pos.y - 20);
            return null;
          });
        }
        return null;
      });
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  };

  const nodeEl = (n: WorkflowNode) => {
    const meta = TYPE_META[n.type];
    const selected = selectedNodeId === n.id;
    const isAgentNode = n.type === 'agent' || n.type === 'subagent' || n.type === 'checker';
    const nodes = current?.nodes ?? [];
    return (
      <div
        key={n.id}
        className={`absolute z-[2] w-[200px] cursor-grab rounded-[10px] border bg-panel p-3 text-left shadow-[0_12px_30px_#0007] transition-shadow hover:shadow-[0_18px_36px_#0009] ${selected ? 'border-[var(--green)]' : 'border-line'}`}
        style={{ left: n.x, top: n.y, borderTop: `2px solid ${meta.color}` }}
        onPointerDown={(e) => {
          e.stopPropagation();
          const rect = canvasRef.current?.getBoundingClientRect();
          if (!rect) return;
          setSelectedNodeId(n.id);
          const dx = e.clientX - rect.left - n.x, dy = e.clientY - rect.top - n.y;
          setMoving({ id: n.id, dx, dy });
          const onMove = (ev: PointerEvent) => {
            const r = canvasRef.current?.getBoundingClientRect();
            if (!r) return;
            moveNode(n.id, Math.round(ev.clientX - r.left - dx), Math.round(ev.clientY - r.top - dy));
          };
          const onUp = () => {
            window.removeEventListener('pointermove', onMove);
            window.removeEventListener('pointerup', onUp);
            setMoving(null);
          };
          window.addEventListener('pointermove', onMove);
          window.addEventListener('pointerup', onUp);
        }}
      >
        <div className="flex items-center gap-1.5">
          <span style={{ color: meta.color }}>{meta.icon}</span>
          <b className="flex-1 truncate text-[12.5px]">{n.label}</b>
          <button className="cursor-pointer border-0 bg-transparent p-0 text-muted hover:text-[#f87171]" onClick={(e) => { e.stopPropagation(); deleteNode(n.id); }}><X size={11} /></button>
        </div>
        <div className="mt-2 truncate text-[11px] text-muted">
          {isAgentNode ? (n.agentId ? agentName(n.agentId) : 'No agent assigned') : meta.label}
        </div>
        {/* Output port */}
        <div
          className="absolute -right-[7px] top-1/2 z-[3] h-3.5 w-3.5 -translate-y-1/2 cursor-crosshair rounded-full border-2 border-line bg-panel2 hover:border-[var(--green)]"
          onPointerDown={(e) => {
            e.stopPropagation(); e.preventDefault();
            setDrawingEdge({ from: n.id, x: n.x + NODE_W, y: n.y + NODE_H / 2 });
            const onMove = (ev: PointerEvent) => {
              const rect = canvasRef.current?.getBoundingClientRect();
              if (!rect) return;
              setDrawingEdge({ from: n.id, x: ev.clientX - rect.left, y: ev.clientY - rect.top });
            };
            const onUp = (ev: PointerEvent) => {
              window.removeEventListener('pointermove', onMove);
              window.removeEventListener('pointerup', onUp);
              // Hit-test which node is under the release point.
              const rect = canvasRef.current?.getBoundingClientRect();
              if (!rect) { setDrawingEdge(null); return; }
              const px = ev.clientX - rect.left, py = ev.clientY - rect.top;
              let target: string | null = null;
              for (const other of nodes) {
                if (other.id === n.id) continue;
                if (px >= other.x && px <= other.x + NODE_W && py >= other.y && py <= other.y + NODE_H) { target = other.id; break; }
              }
              if (target) {
                const edge: WorkflowEdge = { id: `e-${Date.now()}`, from: n.id, to: target };
                update((w) => ({ ...w, edges: [...w.edges, edge] }));
              }
              setDrawingEdge(null);
            };
            window.addEventListener('pointermove', onMove);
            window.addEventListener('pointerup', onUp);
          }}
          title="Drag to connect"
        />
      </div>
    );
  };

  return (
    <div className="flex h-full gap-4">
      {/* Left rail: workflow list */}
      <div className="w-[230px] shrink-0 overflow-y-auto rounded-[10px] border border-line bg-panel p-3.5">
        <div className="mb-3 flex items-center justify-between">
          <span className="font-mono text-[10px] tracking-[1px] text-muted">WORKFLOWS</span>
          <button className="cursor-pointer border-0 bg-transparent p-0 text-muted hover:text-text" onClick={() => setCurrent({ id: `wf-${Date.now()}`, name: 'Untitled workflow', nodes: [], edges: [], updatedAt: new Date().toISOString() })}><Plus size={13} /></button>
        </div>
        {workflows.map((w) => (
          <div key={w.id} className="mb-1.5 flex items-center gap-1.5">
            <button
              className={`flex-1 cursor-pointer rounded-[6px] border-0 px-2.5 py-2 text-left text-[12.5px] ${current?.id === w.id ? 'bg-panel2 text-text' : 'text-muted hover:bg-line'}`}
              onClick={() => { setCurrent(w); setRunResult(null); setSelectedNodeId(null); }}
            >
              <WorkflowIcon size={11} className="mr-1.5 inline-block" />{w.name}
            </button>
            <button className="cursor-pointer border-0 bg-transparent p-1 text-muted hover:text-[#f87171]" onClick={async () => { await onDeleteWorkflow(w.id); toast('Workflow deleted', 'success'); }}><Trash2 size={11} /></button>
          </div>
        ))}
        {workflows.length === 0 && <p className="px-2 text-[12px] leading-1.6 text-muted">No workflows yet. Click + to create one.</p>}
      </div>

      {/* Main: builder */}
      {current ? (
        <div className="flex min-w-0 flex-1 flex-col gap-3">
          <header className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex min-w-0 items-center gap-2">
              <span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span>
              <input
                value={current.name}
                onChange={(e) => update((w) => ({ ...w, name: e.target.value }))}
                className="w-48 rounded-md border border-line bg-panel2 px-2 py-1 text-[13px] text-text outline-none focus:border-mid"
              />
            </div>
            <div className="flex items-center gap-2">
              <input
                value={runInput}
                onChange={(e) => setRunInput(e.target.value)}
                placeholder="Workflow input…"
                className="w-56 rounded-md border border-line bg-panel2 px-2.5 py-1.5 text-[12px] text-text outline-none placeholder:text-muted focus:border-mid"
              />
              <button className="secondary" onClick={save} disabled={saving}><Save size={13} />{saving ? 'Saving…' : 'Save'}</button>
              <button className="primary" onClick={run} disabled={running}><Play size={13} />{running ? 'Running…' : 'Run'}</button>
            </div>
          </header>

          <div className="flex min-h-0 flex-1 gap-3">
            {/* Palette */}
            <div className="w-[170px] shrink-0 overflow-y-auto rounded-[10px] border border-line bg-panel p-2.5">
              <span className="mb-2.5 block font-mono text-[10px] tracking-[1px] text-muted">NODES</span>
              {NODE_TYPES.map((nt) => (
                <div
                  key={nt.type}
                  className="mb-1.5 cursor-grab rounded-[6px] border border-line bg-panel2 p-2.5 transition-colors hover:border-mid active:cursor-grabbing"
                  onPointerDown={startPaletteDrag(nt.type)}
                  title={nt.desc}
                >
                  <div className="flex items-center gap-1.5 text-[12px]" style={{ color: nt.color }}>
                    {nt.icon}<b className="text-text">{nt.label}</b>
                  </div>
                  <p className="mt-1 text-[10.5px] leading-1.5 text-muted">{nt.desc}</p>
                </div>
              ))}
            </div>

            {/* Canvas */}
            <div
              ref={canvasRef}
              className="relative min-h-[550px] flex-1 overflow-hidden rounded-[12px] border border-line bg-[#0b0b0d]"
              style={{ backgroundImage: 'radial-gradient(#2b2b30 1px, transparent 1px)', backgroundSize: '24px 24px' }}
              onPointerMove={onPointerMove}
              onPointerUp={onPointerUp}
              onPointerLeave={() => { setDragPos(null); setDrawingEdge(null); }}
            >
              <svg className="pointer-events-none absolute inset-0 z-0 h-full w-full">
                {current.edges.map((e) => {
                  const from = current.nodes.find((n) => n.id === e.from);
                  const to = current.nodes.find((n) => n.id === e.to);
                  if (!from || !to) return null;
                  const x1 = from.x + NODE_W, y1 = from.y + NODE_H / 2;
                  const x2 = to.x, y2 = to.y + NODE_H / 2;
                  return <path key={e.id} className="fill-none stroke-dim stroke-[1.5]" d={`M ${x1} ${y1} C ${x1 + 40} ${y1}, ${x2 - 40} ${y2}, ${x2} ${y2}`} />;
                })}
                {drawingEdge && <path className="fill-none stroke-[var(--green)] stroke-[1.5] [stroke-dasharray:5_5]" d={`M ${drawingEdge.x} ${drawingEdge.y} C ${drawingEdge.x + 40} ${drawingEdge.y}, ${drawingEdge.x - 40} ${drawingEdge.y}, ${drawingEdge.x} ${drawingEdge.y}`} />}
              </svg>

              {current.nodes.map(nodeEl)}

              {drag && dragPos && (
                <div className="pointer-events-none absolute z-[5] w-[200px] rounded-[10px] border border-[var(--green)] border-dashed bg-panel/80 p-3 opacity-80" style={{ left: dragPos.x - NODE_W / 2, top: dragPos.y - 20 }}>
                  <b className="text-[12px]">{TYPE_META[drag.type].label}</b>
                </div>
              )}

              {current.nodes.length === 0 && (
                <div className="absolute inset-0 grid place-items-center">
                  <div className="text-center text-muted">
                    <WorkflowIcon size={26} className="mx-auto mb-2 opacity-50" />
                    <p className="text-[12px]">Drag nodes from the palette onto the canvas.</p>
                    <p className="text-[11px]">Connect ports (dots on node edges) to chain agents.</p>
                  </div>
                </div>
              )}
            </div>

            {/* Inspector / results */}
            <div className="w-[260px] shrink-0 overflow-y-auto rounded-[10px] border border-line bg-panel p-3.5">
              {runResult ? (
                <div>
                  <div className="mb-2.5 flex items-center justify-between">
                    <span className="font-mono text-[10px] tracking-[1px] text-muted">RESULTS</span>
                    <button className="cursor-pointer border-0 bg-transparent p-0 text-muted hover:text-text" onClick={() => setRunResult(null)}><X size={12} /></button>
                  </div>
                  <p className="mb-3 text-[12px] text-muted">{(runResult.totalPromptTokens + runResult.totalCompletionTokens).toLocaleString()} tokens</p>
                  <div className="grid gap-2.5">
                    {runResult.steps.map((s, i) => (
                      <div key={i} className="rounded-[6px] border border-line bg-panel2 p-2.5">
                        <b className="text-[12px]">{i + 1}. {s.nodeLabel}</b>
                        <p className="mt-1.5 max-h-24 overflow-y-auto text-[11px] leading-1.6 text-muted">{s.output}</p>
                      </div>
                    ))}
                  </div>
                  <div className="mt-3 rounded-[6px] border border-[var(--green)] bg-panel2 p-2.5">
                    <b className="text-[12px]">Final output</b>
                    <p className="mt-1.5 max-h-40 overflow-y-auto text-[11px] leading-1.6 text-muted">{runResult.finalOutput}</p>
                  </div>
                </div>
              ) : runError ? (
                <div>
                  <span className="font-mono text-[10px] tracking-[1px] text-[#f87171]">ERROR</span>
                  <p className="mt-2 text-[12px] leading-1.6 text-[#f87171]">{runError}</p>
                </div>
              ) : selectedNodeId ? (() => {
                const node = current.nodes.find((n) => n.id === selectedNodeId);
                if (!node) return null;
                const isAgentNode = node.type === 'agent' || node.type === 'subagent' || node.type === 'checker';
                return (
                  <div>
                    <div className="mb-2.5 flex items-center justify-between">
                      <span className="font-mono text-[10px] tracking-[1px] text-muted">NODE</span>
                      <button className="cursor-pointer border-0 bg-transparent p-0 text-muted hover:text-[#f87171]" onClick={() => deleteNode(node.id)}><Trash2 size={12} /></button>
                    </div>
                    <label className="mb-1.5 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">LABEL</label>
                    <input value={node.label} onChange={(e) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, label: e.target.value } : n)) }))} className="w-full rounded-md border border-line bg-panel2 px-2.5 py-2 text-[12.5px] text-text outline-none focus:border-mid" />
                    {isAgentNode && (
                      <>
                        <label className="mb-1.5 mt-4 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">AGENT</label>
                        <Dropdown
                          value={node.agentId ?? ''}
                          options={agents.map((a) => ({ value: a.id, label: a.name }))}
                          onChange={(v) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, agentId: v } : n)) }))}
                          placeholder="Select agent…"
                        />
                      </>
                    )}
                    {node.type === 'loop' && (
                      <>
                        <label className="mb-1.5 mt-4 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">MAX ITERATIONS</label>
                        <input type="number" min={1} value={String(node.config?.maxIterations ?? 3)} onChange={(e) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, config: { ...n.config, maxIterations: Number(e.target.value) } } : n)) }))} className="w-full rounded-md border border-line bg-panel2 px-2.5 py-2 text-[12.5px] text-text outline-none focus:border-mid" />
                      </>
                    )}
                    <p className="mt-4 text-[11px] leading-1.6 text-muted">{TYPE_META[node.type].label} node. Drag its port to connect output to another node.</p>
                  </div>
                );
              })() : (
                <div className="text-center text-muted">
                  <p className="text-[12px] leading-1.6">Select a node to edit its config, or drag from a node's port to create a connection.</p>
                </div>
              )}
            </div>
          </div>
        </div>
      ) : (
        <div className="grid flex-1 place-items-center rounded-[12px] border border-dashed border-soft">
          <div className="text-center text-muted">
            <WorkflowIcon size={30} className="mx-auto mb-2 opacity-50" />
            <h3 className="mb-1 text-text">Workflow Builder</h3>
            <p className="text-[12px]">Create or select a workflow to start building.</p>
          </div>
        </div>
      )}
    </div>
  );
}
