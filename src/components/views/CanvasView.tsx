import { useEffect, useRef, useState } from 'react';
import { ArrowLeft, Bot, Check, GitBranch, Play, Plus, Repeat, Save, ShieldCheck, Trash2, Webhook, Workflow as WorkflowIcon, X } from 'lucide-react';
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

const fmtDate = (s?: string) => {
  if (!s) return '';
  const d = new Date(s);
  return isNaN(d.getTime()) ? '' : d.toLocaleDateString();
};

export function CanvasView({ agents, tools, workflows, onSaveWorkflow, onDeleteWorkflow, onRunWorkflow, initialWorkflowId, onInitialWorkflowConsumed }: {
  agents: Agent[]; tools: Tool[]; workflows: Workflow[];
  onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
  onDeleteWorkflow: (id: string) => Promise<void>;
  onRunWorkflow: (w: Workflow, input: string) => Promise<WorkflowRunResult>;
  initialWorkflowId?: string | null;
  onInitialWorkflowConsumed?: () => void;
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
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [zoom, setZoom] = useState(1);
  const [panning, setPanning] = useState<{ startX: number; startY: number; origX: number; origY: number } | null>(null);
  const [search, setSearch] = useState('');
  const [confirmTarget, setConfirmTarget] = useState<Workflow | null>(null);
  const [typedName, setTypedName] = useState('');
  const canvasRef = useRef<HTMLDivElement>(null);

  const update = (fn: (w: Workflow) => Workflow) => {
    setCurrent((prev) => prev ? fn(prev) : prev);
  };

  // Convert a pointer position (relative to the canvas element) into world
  // coordinates, so node placement/dragging/edge drawing all match the
  // panned + zoomed view.
  const screenToWorld = (sx: number, sy: number) => ({ x: (sx - pan.x) / zoom, y: (sy - pan.y) / zoom });

  useEffect(() => {
    if (!initialWorkflowId) return;
    if (initialWorkflowId === 'new') {
      createNew();
    } else {
      const wf = workflows.find((w) => w.id === initialWorkflowId);
      if (wf) openWorkflow(wf);
    }
    onInitialWorkflowConsumed?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialWorkflowId]);

  const openWorkflow = (w: Workflow) => {
    setCurrent(w); setRunResult(null); setRunError(null); setSelectedNodeId(null); setPan({ x: 0, y: 0 });
  };

  const createNew = () => {
    setCurrent({ id: `wf-${Date.now()}`, name: 'Untitled workflow', nodes: [], edges: [], updatedAt: new Date().toISOString() });
    setRunResult(null); setRunError(null); setSelectedNodeId(null); setPan({ x: 0, y: 0 });
  };

  const closeBuilder = () => {
    setCurrent(null); setSelectedNodeId(null); setRunResult(null); setRunError(null); setDraft(null); setPan({ x: 0, y: 0 });
  };

  const deleteWorkflow = async (id: string) => {
    await onDeleteWorkflow(id);
    if (current?.id === id) closeBuilder();
    toast('Workflow deleted', 'success');
  };

  const openConfirm = (w: Workflow) => { setConfirmTarget(w); setTypedName(''); };
  const closeConfirm = () => { setConfirmTarget(null); setTypedName(''); };

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
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    if (drag) setDragPos({ x: sx, y: sy });
    if (moving) {
      const w = screenToWorld(sx, sy);
      moveNode(moving.id, Math.round(w.x - moving.dx), Math.round(w.y - moving.dy));
    }
    if (drawingEdge) setDrawingEdge({ from: drawingEdge.from, x: sx, y: sy });
    if (panning) {
      setPan({ x: panning.origX + (e.clientX - panning.startX), y: panning.origY + (e.clientY - panning.startY) });
    }
  };

  const onPointerUp = () => {
    if (drag && dragPos) {
      const w = screenToWorld(dragPos.x, dragPos.y);
      addNode(drag.type, Math.round(w.x - NODE_W / 2), Math.round(w.y - 20));
    }
    setDrag(null); setDragPos(null); setMoving(null); setDrawingEdge(null); setPanning(null);
  };

  const onCanvasPointerDown = (e: React.PointerEvent) => {
    // Pan when clicking any empty space. Nodes/ports call stopPropagation on
    // their own pointerdown, so whatever reaches here is background.
    if (drag || moving || drawingEdge) return;
    e.preventDefault();
    // Capture the pointer so panning keeps working even if the cursor leaves
    // the canvas and releases outside it.
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    setPanning({ startX: e.clientX, startY: e.clientY, origX: pan.x, origY: pan.y });
  };

  // Wheel = zoom toward the cursor.
  const onCanvasWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
    const next = Math.min(Math.max(zoom * factor, 0.25), 3);
    if (next === zoom) return;
    // Keep the world point under the cursor stationary.
    const wx = (sx - pan.x) / zoom;
    const wy = (sy - pan.y) / zoom;
    setZoom(next);
    setPan({ x: sx - wx * next, y: sy - wy * next });
  };

  const zoomIn = () => {
    const rect = canvasRef.current?.getBoundingClientRect();
    const cx = rect ? rect.width / 2 : 0;
    const cy = rect ? rect.height / 2 : 0;
    const wx = (cx - pan.x) / zoom, wy = (cy - pan.y) / zoom;
    const next = Math.min(zoom * 1.2, 3);
    setZoom(next);
    setPan({ x: cx - wx * next, y: cy - wy * next });
  };

  const zoomOut = () => {
    const rect = canvasRef.current?.getBoundingClientRect();
    const cx = rect ? rect.width / 2 : 0;
    const cy = rect ? rect.height / 2 : 0;
    const wx = (cx - pan.x) / zoom, wy = (cy - pan.y) / zoom;
    const next = Math.max(zoom / 1.2, 0.25);
    setZoom(next);
    setPan({ x: cx - wx * next, y: cy - wy * next });
  };

  const resetView = () => { setZoom(1); setPan({ x: 0, y: 0 }); };

  const startPaletteDrag = (type: WorkflowNodeType) => (e: React.PointerEvent) => {
    e.preventDefault();
    setDrag({ type });
    setDragPos(null);
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
            if (pos) {
              const w = screenToWorld(pos.x, pos.y);
              addNode(d.type, Math.round(w.x - NODE_W / 2), Math.round(w.y - 20));
            }
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
          const w = screenToWorld(e.clientX - rect.left, e.clientY - rect.top);
          const dx = w.x - n.x, dy = w.y - n.y;
          setMoving({ id: n.id, dx, dy });
          const onMove = (ev: PointerEvent) => {
            const r = canvasRef.current?.getBoundingClientRect();
            if (!r) return;
            const w2 = screenToWorld(ev.clientX - r.left, ev.clientY - r.top);
            moveNode(n.id, Math.round(w2.x - dx), Math.round(w2.y - dy));
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
        <div
          className="absolute -right-[7px] top-1/2 z-[3] h-3.5 w-3.5 -translate-y-1/2 cursor-crosshair rounded-full border-2 border-line bg-panel2 hover:border-[var(--green)]"
          onPointerDown={(e) => {
            e.stopPropagation(); e.preventDefault();
            const rect0 = canvasRef.current?.getBoundingClientRect();
            if (!rect0) return;
            const w0 = screenToWorld(e.clientX - rect0.left, e.clientY - rect0.top);
            setDrawingEdge({ from: n.id, x: w0.x + NODE_W, y: w0.y + NODE_H / 2 });
            const onMove = (ev: PointerEvent) => {
              const rect = canvasRef.current?.getBoundingClientRect();
              if (!rect) return;
              const wm = screenToWorld(ev.clientX - rect.left, ev.clientY - rect.top);
              setDrawingEdge({ from: n.id, x: wm.x, y: wm.y });
            };
            const onUp = (ev: PointerEvent) => {
              window.removeEventListener('pointermove', onMove);
              window.removeEventListener('pointerup', onUp);
              const rect = canvasRef.current?.getBoundingClientRect();
              if (!rect) { setDrawingEdge(null); return; }
              const wu = screenToWorld(ev.clientX - rect.left, ev.clientY - rect.top);
              const px = wu.x, py = wu.y;
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

  // ---------------- Builder screen ----------------
  if (current) {
    return (
      <div className="flex h-full min-h-0 flex-col gap-3">
        <header className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2.5">
            <button className="flex cursor-pointer items-center gap-1 rounded-[6px] border-0 bg-none px-1.5 py-1 font-mono text-[10px] uppercase tracking-[1px] text-muted hover:text-text" onClick={closeBuilder}><ArrowLeft size={12} />Workflows</button>
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

          <div
            ref={canvasRef}
            className="relative min-h-[550px] flex-1 overflow-hidden rounded-[12px] border border-line bg-[#0b0b0d]"
            style={{ backgroundImage: 'radial-gradient(#2b2b30 1px, transparent 1px)', backgroundSize: '24px 24px' }}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerDown={onCanvasPointerDown}
            onWheel={onCanvasWheel}
            onPointerLeave={() => { setDragPos(null); setDrawingEdge(null); }}
          >
            {/* Panned + zoomed world layer */}
            <div
              className="absolute inset-0 z-[1] origin-top-left"
              style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}
            >
              <svg className="pointer-events-none absolute top-0 left-0 z-0 h-full w-full">
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

              {drag && dragPos && (() => {
                const w = screenToWorld(dragPos.x, dragPos.y);
                return (
                  <div className="pointer-events-none absolute z-[5] w-[200px] rounded-[10px] border border-[var(--green)] border-dashed bg-panel/80 p-3 opacity-80" style={{ left: w.x - NODE_W / 2, top: w.y - 20 }}>
                    <b className="text-[12px]">{TYPE_META[drag.type].label}</b>
                  </div>
                );
              })()}
            </div>

            {current.nodes.length === 0 && (
              <div className="pointer-events-none absolute inset-0 z-0 grid place-items-center">
                <div className="text-center text-muted">
                  <WorkflowIcon size={26} className="mx-auto mb-2 opacity-50" />
                  <p className="text-[12px]">Drag nodes from the palette onto the canvas.</p>
                  <p className="text-[11px]">Scroll to zoom · drag empty space to pan · connect ports to chain agents.</p>
                </div>
              </div>
            )}

            {/* Zoom controls */}
            <div className="absolute bottom-3 left-3 z-[6] flex items-center gap-1 rounded-lg border border-line bg-panel p-1 shadow-[0_8px_24px_#0008]">
              <button className="grid h-7 w-7 cursor-pointer place-items-center rounded-md border-0 bg-transparent text-muted hover:bg-line hover:text-text" onClick={zoomOut} title="Zoom out">−</button>
              <span className="w-10 text-center font-mono text-[10px] text-muted">{Math.round(zoom * 100)}%</span>
              <button className="grid h-7 w-7 cursor-pointer place-items-center rounded-md border-0 bg-transparent text-muted hover:bg-line hover:text-text" onClick={zoomIn} title="Zoom in">+</button>
              <button className="cursor-pointer rounded-md border-0 bg-transparent px-2 py-1 text-[10px] text-muted hover:bg-line hover:text-text" onClick={resetView} title="Reset view">Reset</button>
            </div>
          </div>

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
    );
  }

  // ---------------- Workflow list screen ----------------
  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex flex-wrap items-center justify-between gap-2" style={{ marginBottom: 24 }}>
        <div>
          <span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span>
          <h1 style={{ margin: 0, fontSize: 24 }}>Workflows</h1>
        </div>
        <div className="flex items-center gap-2">
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search workflows…"
            className="w-52 rounded-md border border-line bg-panel2 px-2.5 py-1.5 text-[12px] text-text outline-none placeholder:text-muted focus:border-mid"
          />
          <button className="primary" onClick={createNew}><Plus size={13} />New workflow</button>
        </div>
      </header>

      {(() => {
        const q = search.trim().toLowerCase();
        const filtered = q ? workflows.filter((w) => w.name.toLowerCase().includes(q)) : workflows;
        if (workflows.length === 0) {
          return (
            <div className="grid flex-1 place-items-center rounded-[12px] border border-dashed border-soft">
              <div className="text-center text-muted">
                <WorkflowIcon size={30} className="mx-auto mb-2 opacity-50" />
                <h3 className="mb-1 text-text">No workflows yet</h3>
                <p className="text-[12px]">Create your first workflow to chain agents and tools.</p>
              </div>
            </div>
          );
        }
        if (filtered.length === 0) {
          return (
            <div className="grid flex-1 place-items-center rounded-[12px] border border-dashed border-soft">
              <div className="text-center text-muted">
                <p className="text-[12px]">No workflows match "{search}".</p>
              </div>
            </div>
          );
        }
        return (
          <div className="grid max-w-[1100px] grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {filtered.map((w) => (
              <div key={w.id} className="group flex flex-col rounded-[10px] border border-line bg-panel p-[18px]">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex min-w-0 items-start gap-3">
                    <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-panel2 text-muted"><WorkflowIcon size={15} /></span>
                    <div className="min-w-0">
                      <b className="block truncate text-[13px]">{w.name}</b>
                      <span className="mt-0.5 block text-[11px] text-muted">{w.nodes.length} nodes · {w.edges.length} connections</span>
                    </div>
                  </div>
                  <button className="cursor-pointer border-0 bg-transparent p-1 text-muted opacity-0 transition-opacity hover:text-[#f87171] group-hover:opacity-100" onClick={() => openConfirm(w)} title="Delete workflow"><Trash2 size={13} /></button>
                </div>
                <span className="mt-2 block font-mono text-[9px] text-muted">updated {fmtDate(w.updatedAt)}</span>
                <div className="mt-4 flex justify-end">
                  <button className="secondary px-3 py-1.5 text-[11px]" onClick={() => openWorkflow(w)}>Open builder</button>
                </div>
              </div>
            ))}
          </div>
        );
      })()}

      {confirmTarget && (
        <div className="fixed inset-0 z-[90] grid place-items-center bg-black/60" onClick={closeConfirm}>
          <div className="w-[380px] max-w-[92vw] rounded-[12px] border border-line bg-panel p-5 shadow-[0_20px_60px_#000a]" onClick={(e) => e.stopPropagation()}>
            <div className="mb-1 flex items-center gap-2">
              <Trash2 size={15} className="text-[#f87171]" />
              <b className="text-[14px]">Delete "{confirmTarget.name}"?</b>
            </div>
            <p className="mb-4 text-[12px] leading-1.6 text-muted">This permanently removes the workflow and its connections. Type the workflow's name to confirm.</p>
            <input
              value={typedName}
              onChange={(e) => setTypedName(e.target.value)}
              placeholder={confirmTarget.name}
              className="mb-4 w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[13px] text-text outline-none focus:border-mid"
              autoFocus
            />
            <div className="flex justify-end gap-2">
              <button className="secondary" onClick={closeConfirm}>Cancel</button>
              <button
                className="primary"
                style={{ background: '#e11d48', color: '#fff' }}
                disabled={typedName.trim() !== confirmTarget.name}
                onClick={() => { deleteWorkflow(confirmTarget.id); closeConfirm(); }}
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
