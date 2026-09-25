import { useEffect, useRef, useState } from 'react';
import { Bot, GitBranch, Play, Plus, Repeat, RotateCcw, Save, ShieldCheck, Trash2, Webhook, Workflow as WorkflowIcon, X, ZoomIn, ZoomOut } from 'lucide-react';
import type { Agent, Integration, Tool, Workflow, WorkflowEdge, WorkflowNode, WorkflowNodeType, WorkflowRunResult } from '../../types';
import { Select } from '../ui/Select';
import { DeleteConfirm } from '../ui/DeleteConfirm';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { StatusGlyph, statusClass } from '../ui/Status';
import { Tooltip } from '../ui/Tooltip';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS, INPUT_INLINE_CLS, PROSE_CLS } from '../ui/Input';
import { toast } from '../../hooks/useToast';
import { listWorkflowRuns, type WorkflowRunRecord } from '../../runtime';

// Safe timestamp formatting — DB values can be empty or malformed.
const fmtWhen = (s?: string | null) => {
  if (!s) return '';
  const d = new Date(s);
  return isNaN(d.getTime()) ? '' : d.toLocaleString();
};

// Node types carry an icon and a label — never a per-type hue. A type is
// legible from its glyph; colour is reserved for the accent.
const NODE_TYPES: { type: WorkflowNodeType; label: string; icon: React.ReactNode; desc: string }[] = [
  { type: 'trigger', label: 'trigger', icon: <Webhook size={13} />, desc: 'Workflow entry point' },
  { type: 'agent', label: 'agent', icon: <Bot size={13} />, desc: 'Run an agent' },
  { type: 'subagent', label: 'subagent', icon: <GitBranch size={13} />, desc: 'Delegate to a sub-agent' },
  { type: 'loop', label: 'loop', icon: <Repeat size={13} />, desc: 'Repeat until done' },
  { type: 'checker', label: 'checker', icon: <ShieldCheck size={13} />, desc: 'Validate output' },
  { type: 'integration', label: 'integration', icon: <Webhook size={13} />, desc: 'Call a tool/integration' },
  { type: 'gate', label: 'gate', icon: <GitBranch size={13} />, desc: 'Conditional branch' },
];

const TYPE_META: Record<WorkflowNodeType, { label: string; icon: React.ReactNode }> = Object.fromEntries(
  NODE_TYPES.map((n) => [n.type, { label: n.label, icon: n.icon }])
) as Record<WorkflowNodeType, { label: string; icon: React.ReactNode }>;

const NODE_W = 200;
const NODE_H = 84;
const INSPECTOR_W = 300;

// Rule types available in the checker builder, with their editable fields.
const RULE_TYPES: { type: string; label: string; fields: { key: string; label: string; placeholder?: string }[] }[] = [
  { type: 'contains', label: 'contains', fields: [{ key: 'value', label: 'text', placeholder: 'TODO' }] },
  { type: 'notContains', label: 'does not contain', fields: [{ key: 'value', label: 'text', placeholder: 'TODO' }] },
  { type: 'nonEmpty', label: 'non-empty', fields: [] },
  { type: 'startsWith', label: 'starts with', fields: [{ key: 'value', label: 'prefix' }] },
  { type: 'endsWith', label: 'ends with', fields: [{ key: 'value', label: 'suffix' }] },
  { type: 'regex', label: 'regex', fields: [{ key: 'pattern', label: 'pattern', placeholder: '\\d{4}-\\d{2}-\\d{2}' }, { key: 'mode', label: 'mode' }] },
  { type: 'validJson', label: 'valid json', fields: [] },
  { type: 'equals', label: 'equals', fields: [{ key: 'value', label: 'expected' }] },
  { type: 'notEquals', label: 'not equals', fields: [{ key: 'value', label: 'value' }] },
  { type: 'lengthRange', label: 'length range', fields: [{ key: 'min', label: 'min', placeholder: '0' }, { key: 'max', label: 'max', placeholder: '1000' }] },
  { type: 'numericRange', label: 'numeric range', fields: [{ key: 'min', label: 'min' }, { key: 'max', label: 'max' }] },
  { type: 'hasField', label: 'has field', fields: [{ key: 'path', label: 'path', placeholder: 'items[0].title' }, { key: 'min', label: 'min count (arrays)' }] },
  { type: 'fieldType', label: 'field type', fields: [{ key: 'path', label: 'path' }, { key: 'type', label: 'type' }] },
  { type: 'arrayLength', label: 'array length', fields: [{ key: 'path', label: 'path' }, { key: 'min', label: 'min' }, { key: 'max', label: 'max' }] },
  { type: 'inList', label: 'in list', fields: [{ key: 'values', label: 'values (comma sep)' }] },
  { type: 'llmJudge', label: 'llm judge', fields: [{ key: 'prompt', label: 'criteria' }, { key: 'model', label: 'model', placeholder: 'auto' }] },
];

const label = `${FIELD_LABEL_CLS} mt-4`;

function CheckerRules({ config, update }: {
  config: Record<string, unknown>;
  update: (fn: (cfg: Record<string, unknown>) => Record<string, unknown>) => void;
}) {
  const rules = (config.rules as { type: string; label?: string; [k: string]: unknown }[]) ?? [];
  const setRules = (next: unknown[]) => update((cfg) => ({ ...cfg, rules: next }));

  const addRule = (type: string) => {
    const def = RULE_TYPES.find((r) => r.type === type);
    if (!def) return;
    const rule: Record<string, unknown> = { type };
    for (const f of def.fields) {
      if (f.key === 'values') rule.values = [];
      else if (f.key === 'mode') rule.mode = 'mustMatch';
      else if (f.key === 'type') rule.type = 'string'; // fieldType uses `type`
      else if (f.key === 'min') rule.min = 0;
      else if (f.key === 'max') rule.max = 1000;
      else if (f.key === 'model') rule.model = 'auto';
      else rule[f.key] = '';
    }
    setRules([...rules, rule]);
  };

  const updateRule = (i: number, patch: Record<string, unknown>) => {
    const next = rules.map((r, idx) => (idx === i ? { ...r, ...patch } : r));
    setRules(next);
  };

  const removeRule = (i: number) => setRules(rules.filter((_, idx) => idx !== i));

  const fieldValue = (rule: Record<string, unknown>, key: string) => {
    if (key === 'values') return Array.isArray(rule.values) ? (rule.values as string[]).join(', ') : '';
    return rule[key] as string ?? '';
  };

  return (
    <div className="mt-4">
      <div className="mb-1.5 flex items-center justify-between gap-2">
        <label className={`${FIELD_LABEL_CLS} mb-0`}>rules</label>
        <Select
          value=""
          placeholder="+ add rule…"
          options={RULE_TYPES.map((r) => ({ value: r.type, label: r.label }))}
          onChange={(v) => { if (v) addRule(v); }}
        />
      </div>

      {rules.length === 0 && <p className="m-0 font-mono text-[10px] leading-relaxed text-muted">No rules. Add one — all rules must pass (or use an LLM judge for subjective checks).</p>}

      <div className="grid gap-2">
        {rules.map((rule, i) => {
          const def = RULE_TYPES.find((r) => r.type === rule.type);
          return (
            <div key={i} className="rounded-lg border border-border bg-background p-2">
              <div className="mb-1.5 flex items-center justify-between gap-2">
                <span className="min-w-0 truncate font-mono text-[10px] text-foreground">{def?.label ?? rule.type}</span>
                <IconButton label="remove rule" className="h-5 w-5" onClick={() => removeRule(i)}><X size={10} /></IconButton>
              </div>
              {def?.fields.map((f) => (
                <label key={f.key} className="mb-1.5 block last:mb-0">
                  <span className="mb-0.5 block font-mono text-[10px] tracking-wider text-muted uppercase">{f.label}</span>
                  {f.key === 'mode' ? (
                    <Select
                      value={String(rule.mode ?? 'mustMatch')}
                      options={[{ value: 'mustMatch', label: 'must match' }, { value: 'mustNotMatch', label: 'must NOT match' }]}
                      onChange={(v) => updateRule(i, { mode: v })}
                    />
                  ) : f.key === 'type' ? (
                    <Select
                      value={String(rule.type ?? 'string')}
                      options={[
                        { value: 'string', label: 'string' },
                        { value: 'number', label: 'number' },
                        { value: 'bool', label: 'boolean' },
                        { value: 'array', label: 'array' },
                        { value: 'object', label: 'object' },
                      ]}
                      onChange={(v) => updateRule(i, { type: v })}
                    />
                  ) : f.key === 'model' ? (
                    <input value={String(rule.model ?? 'auto')} onChange={(e) => updateRule(i, { model: e.target.value })} placeholder={f.placeholder} className={INPUT_CLS} />
                  ) : (
                    <input
                      value={fieldValue(rule, f.key)}
                      onChange={(e) => updateRule(i, f.key === 'values' ? { values: e.target.value.split(',').map((s) => s.trim()).filter(Boolean) } : { [f.key]: e.target.value })}
                      placeholder={f.placeholder}
                      className={INPUT_CLS}
                    />
                  )}
                </label>
              ))}
            </div>
          );
        })}
      </div>

      <label className={label}>mode</label>
      <Select
        value={String(config.mode ?? 'all')}
        options={[{ value: 'all', label: 'all rules must pass' }, { value: 'any', label: 'any rule passes' }]}
        onChange={(v) => update((cfg) => ({ ...cfg, mode: v }))}
      />
    </div>
  );
}

const fmtDate = (s?: string) => {
  if (!s) return '';
  const d = new Date(s);
  return isNaN(d.getTime()) ? '' : d.toLocaleDateString();
};

export function CanvasView({ agents, tools, workflows, integrations, onSaveWorkflow, onDeleteWorkflow, onRunWorkflow, initialWorkflowId, onInitialWorkflowConsumed, onOpenChange, closeRequest }: {
  agents: Agent[]; tools: Tool[]; workflows: Workflow[]; integrations: Integration[];
  onSaveWorkflow: (w: Workflow) => Promise<Workflow>;
  onDeleteWorkflow: (id: string) => Promise<void>;
  onRunWorkflow: (w: Workflow, input: string) => Promise<WorkflowRunResult>;
  initialWorkflowId?: string | null;
  onInitialWorkflowConsumed?: () => void;
  onOpenChange?: (open: { id: string; name: string } | null) => void;
  closeRequest?: number;
}) {
  const [current, setCurrent] = useState<Workflow | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);
  const [runInput, setRunInput] = useState('');
  const [running, setRunning] = useState(false);
  const [runResult, setRunResult] = useState<WorkflowRunResult | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [storedRuns, setStoredRuns] = useState<WorkflowRunRecord[]>([]);
  const [saving, setSaving] = useState(false);
  const [drag, setDrag] = useState<{ type: WorkflowNodeType } | null>(null);
  const [dragPos, setDragPos] = useState<{ x: number; y: number } | null>(null);
  const [moving, setMoving] = useState<{ id: string; dx: number; dy: number } | null>(null);
  const [drawingEdge, setDrawingEdge] = useState<{ from: string; x: number; y: number } | null>(null);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [zoom, setZoom] = useState(1);
  const [inspectorWidth, setInspectorWidth] = useState(INSPECTOR_W);
  const [panning, setPanning] = useState<{ startX: number; startY: number; origX: number; origY: number } | null>(null);
  const [search, setSearch] = useState('');
  const [confirmTarget, setConfirmTarget] = useState<Workflow | null>(null);
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
    setCurrent(w); setRunResult(null); setRunError(null); setSelectedNodeId(null); setSelectedEdgeId(null); setPan({ x: 0, y: 0 });
  };

  // Load this workflow's persisted run history whenever the open workflow changes.
  useEffect(() => {
    if (!current?.id) { setStoredRuns([]); return; }
    listWorkflowRuns(current.id).then(setStoredRuns);
  }, [current?.id]);

  // Replay a stored run in the results panel.
  const showStoredRun = (r: WorkflowRunRecord) => {
    setRunError(null);
    setRunResult({
      steps: r.steps,
      finalOutput: r.finalOutput ?? '',
      totalPromptTokens: r.promptTokens,
      totalCompletionTokens: r.completionTokens,
    });
  };

  const createNew = () => {
    setCurrent({ id: `wf-${Date.now()}`, name: 'Untitled workflow', nodes: [], edges: [], updatedAt: new Date().toISOString() });
    setRunResult(null); setRunError(null); setSelectedNodeId(null); setSelectedEdgeId(null); setPan({ x: 0, y: 0 });
  };

  const closeBuilder = () => {
    setCurrent(null); setSelectedNodeId(null); setSelectedEdgeId(null); setRunResult(null); setRunError(null); setPan({ x: 0, y: 0 });
  };

  // Mirror the open workflow up so the topbar breadcrumb can name it.
  useEffect(() => {
    onOpenChange?.(current ? { id: current.id, name: current.name } : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.id, current?.name]);

  // A tap on the breadcrumb's parent segment asks the builder to close.
  useEffect(() => {
    if (closeRequest) closeBuilder();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [closeRequest]);

  const deleteWorkflow = async (id: string) => {
    await onDeleteWorkflow(id);
    if (current?.id === id) closeBuilder();
    toast('workflow deleted', 'success');
  };

  const openConfirm = (w: Workflow) => setConfirmTarget(w);
  const closeConfirm = () => setConfirmTarget(null);

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
    setSelectedEdgeId(null);
  };

  const deleteEdge = (id: string) => {
    update((w) => ({ ...w, edges: w.edges.filter((e) => e.id !== id) }));
    setSelectedEdgeId(null);
  };

  const agentName = (id?: string) => agents.find((a) => a.id === id)?.name ?? 'select agent…';

  const save = async () => {
    if (!current) return;
    setSaving(true);
    try {
      const saved = await onSaveWorkflow(current);
      setCurrent(saved);
      toast(`workflow "${saved.name}" saved`, 'success');
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
      listWorkflowRuns(current.id).then(setStoredRuns);
      toast(`workflow run complete — ${(result.totalPromptTokens + result.totalCompletionTokens).toLocaleString()} tokens`, 'success');
    } catch (e) {
      setRunError(e instanceof Error ? e.message : String(e));
      toast(e instanceof Error ? e.message : 'workflow run failed', 'error');
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
    setSelectedEdgeId(null);
    setSelectedNodeId(null);
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

  // Drag the inspector's left edge to resize it.
  const startInspectorResize = (e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = inspectorWidth;
    const onMove = (ev: PointerEvent) => {
      // Dragging left grows the panel, dragging right shrinks it.
      setInspectorWidth(Math.min(Math.max(startW + (startX - ev.clientX), 240), 560));
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      document.body.style.cursor = '';
    };
    document.body.style.cursor = 'col-resize';
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  };

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
        className={`absolute z-[2] w-[200px] cursor-grab rounded-xl border bg-surface p-3 text-left transition-colors ${selected ? 'border-accent' : 'border-border hover:border-foreground/30'}`}
        style={{ left: n.x, top: n.y }}
        onPointerDown={(e) => {
          e.stopPropagation();
          const rect = canvasRef.current?.getBoundingClientRect();
          if (!rect) return;
          setSelectedNodeId(n.id);
          setSelectedEdgeId(null);
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
          <span className="shrink-0 text-muted">{meta.icon}</span>
          <b className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{n.label}</b>
          <IconButton label="delete node" className="h-5 w-5" onClick={(e) => { e.stopPropagation(); deleteNode(n.id); }}><X size={10} /></IconButton>
        </div>
        <div className="mt-2 truncate font-mono text-[10px] text-muted">
          {isAgentNode ? (n.agentId ? agentName(n.agentId) : 'no agent assigned') : meta.label}
        </div>
        <Tooltip label="drag to connect">
          <div
            className="absolute -right-[7px] top-1/2 z-[3] h-3.5 w-3.5 -translate-y-1/2 cursor-crosshair rounded-full border-2 border-border bg-background hover:border-accent"
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
          />
        </Tooltip>
      </div>
    );
  };

  // ---------------- Builder screen ----------------
  if (current) {
    return (
      <div className="flex h-full min-h-0 flex-col gap-3">
        <header className="flex flex-wrap items-center justify-between gap-2">
          <input
            value={current.name}
            onChange={(e) => update((w) => ({ ...w, name: e.target.value }))}
            className={`${INPUT_INLINE_CLS} w-48 font-semibold`}
          />
          <div className="flex items-center gap-2">
            <input
              value={runInput}
              onChange={(e) => setRunInput(e.target.value)}
              placeholder="workflow input…"
              className={`${INPUT_CLS} w-56`}
            />
            <Button icon={<Save size={13} />} onClick={save} disabled={saving}>{saving ? 'saving…' : 'save'}</Button>
            <Button variant="primary" icon={<Play size={13} />} onClick={run} disabled={running}>{running ? 'running…' : 'run'}</Button>
          </div>
        </header>

        <div className="flex min-h-0 flex-1 gap-3">
          <div className="w-[170px] shrink-0 overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-surface p-2.5">
            <span className={`${GROUP_LABEL_CLS} mb-2.5 block`}>nodes</span>
            {NODE_TYPES.map((nt) => (
              <Tooltip key={nt.type} label={nt.desc} className="flex w-full">
                <div
                  className="mb-1.5 w-full cursor-grab rounded-lg border border-border bg-background p-2.5 transition-colors hover:border-foreground/30 active:cursor-grabbing"
                  onPointerDown={startPaletteDrag(nt.type)}
                >
                  <div className="flex items-center gap-1.5 font-mono text-[11px] text-foreground">
                    <span className="shrink-0 text-muted">{nt.icon}</span><b className="min-w-0 truncate font-medium">{nt.label}</b>
                  </div>
                  <p className="mt-1 mb-0 font-mono text-[10px] leading-relaxed text-muted">{nt.desc}</p>
                </div>
              </Tooltip>
            ))}
          </div>

          <div
            ref={canvasRef}
            className="relative min-h-[550px] flex-1 overflow-hidden rounded-xl border border-border bg-background"
            style={{ backgroundImage: 'radial-gradient(var(--color-border) 1px, transparent 1px)', backgroundSize: '24px 24px' }}
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
                  const d = `M ${x1} ${y1} C ${x1 + 40} ${y1}, ${x2 - 40} ${y2}, ${x2} ${y2}`;
                  const selected = selectedEdgeId === e.id;
                  return (
                    <g key={e.id} className="pointer-events-auto cursor-pointer" onClick={(ev) => { ev.stopPropagation(); setSelectedEdgeId(e.id); setSelectedNodeId(null); }}>
                      {/* Wide invisible hit area */}
                      <path d={d} className="fill-none stroke-transparent" strokeWidth={14} />
                      <path d={d} className={`fill-none stroke-[1.5] ${selected ? 'stroke-[var(--color-accent)]' : 'stroke-[var(--color-border)]'}`} />
                      {selected && (
                        <g transform={`translate(${(x1 + x2) / 2}, ${(y1 + y2) / 2})`} onClick={(ev) => { ev.stopPropagation(); deleteEdge(e.id); setSelectedEdgeId(null); }}>
                          <circle r={9} className="fill-surface stroke-border" strokeWidth={1.5} />
                          <path d="M -3 -3 L 3 3 M 3 -3 L -3 3" className="stroke-danger" strokeWidth={1.4} strokeLinecap="round" fill="none" />
                        </g>
                      )}
                    </g>
                  );
                })}
                {drawingEdge && <path className="fill-none stroke-[var(--color-accent)] stroke-[1.5] [stroke-dasharray:5_5]" d={`M ${drawingEdge.x} ${drawingEdge.y} C ${drawingEdge.x + 40} ${drawingEdge.y}, ${drawingEdge.x - 40} ${drawingEdge.y}, ${drawingEdge.x} ${drawingEdge.y}`} />}
              </svg>

              {current.nodes.map(nodeEl)}

              {drag && dragPos && (() => {
                const w = screenToWorld(dragPos.x, dragPos.y);
                return (
                  <div className="pointer-events-none absolute z-[5] w-[200px] rounded-xl border border-accent border-dashed bg-surface/80 p-3 opacity-80" style={{ left: w.x - NODE_W / 2, top: w.y - 20 }}>
                    <b className="font-mono text-[11px] text-foreground">{TYPE_META[drag.type].label}</b>
                  </div>
                );
              })()}
            </div>

            {current.nodes.length === 0 && (
              <div className="pointer-events-none absolute inset-0 z-0 grid place-items-center">
                <div className="max-w-[360px] px-4 text-center text-muted">
                  <WorkflowIcon size={24} className="mx-auto mb-2 opacity-50" />
                  <p className="m-0 text-[12px] leading-relaxed">Drag nodes from the palette onto the canvas.</p>
                  <p className="mt-1 mb-0 font-mono text-[10px] leading-relaxed">scroll to zoom · drag empty space to pan · connect ports to chain agents</p>
                </div>
              </div>
            )}

            {/* Zoom controls */}
            <div className="absolute bottom-3 left-3 z-[6] flex items-center gap-0.5 rounded-xl border border-border bg-surface p-1">
              <IconButton label="zoom out" className="h-6 w-6" onClick={zoomOut}><ZoomOut size={13} /></IconButton>
              <span className="w-10 text-center font-mono text-[10px] text-muted">{Math.round(zoom * 100)}%</span>
              <IconButton label="zoom in" className="h-6 w-6" onClick={zoomIn}><ZoomIn size={13} /></IconButton>
              <IconButton label="reset view" className="h-6 w-6" onClick={resetView}><RotateCcw size={13} /></IconButton>
            </div>
          </div>

          {/* Inspector / results — the same panel recipe as the graph inspector */}
          <div className="relative flex shrink-0" style={{ width: inspectorWidth }}>
            <div
              aria-label="drag to resize"
              className="absolute top-0 bottom-0 left-0 z-[7] w-1 cursor-col-resize bg-transparent transition-colors hover:bg-foreground/20"
              onPointerDown={startInspectorResize}
            />
            <div className="flex min-w-0 flex-1 flex-col overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-surface p-3.5">
            {runResult ? (
              <div>
                <div className="mb-2.5 flex items-center justify-between gap-2">
                  <span className={GROUP_LABEL_CLS}>results</span>
                  <IconButton label="close results" className="h-6 w-6" onClick={() => setRunResult(null)}><X size={12} /></IconButton>
                </div>
                <p className="mt-0 mb-3 font-mono text-[10px] text-muted">{(runResult.totalPromptTokens + runResult.totalCompletionTokens).toLocaleString()} tokens</p>
                <div className="grid gap-2.5">
                  {runResult.steps.map((s, i) => (
                    <div key={i} className="rounded-lg border border-border bg-background p-2.5">
                      <b className="font-mono text-[10px] text-foreground">{i + 1}. {s.nodeLabel}</b>
                      <p className="mt-1.5 mb-0 max-h-24 overflow-x-hidden overflow-y-auto text-[11px] leading-relaxed text-muted">{s.output}</p>
                    </div>
                  ))}
                </div>
                <div className="mt-3 rounded-lg border border-accent bg-background p-2.5">
                  <b className="font-mono text-[10px] text-foreground">final output</b>
                  <p className="mt-1.5 mb-0 max-h-40 overflow-x-hidden overflow-y-auto text-[11px] leading-relaxed text-muted">{runResult.finalOutput}</p>
                </div>
              </div>
            ) : runError ? (
              <div>
                <span className={`${GROUP_LABEL_CLS} text-danger`}>error</span>
                <p className="mt-2 mb-0 text-[12px] leading-relaxed text-danger">{runError}</p>
              </div>
            ) : selectedNodeId ? (() => {
              const node = current.nodes.find((n) => n.id === selectedNodeId);
              if (!node) return null;
              const isAgentNode = node.type === 'agent' || node.type === 'subagent' || node.type === 'checker';
              return (
                <div>
                  <div className="mb-2.5 flex items-center justify-between gap-2">
                    <span className={GROUP_LABEL_CLS}>node</span>
                    <IconButton label="delete node" className="h-6 w-6" onClick={() => deleteNode(node.id)}><Trash2 size={12} /></IconButton>
                  </div>
                  <label className={FIELD_LABEL_CLS}>label</label>
                  <input value={node.label} onChange={(e) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, label: e.target.value } : n)) }))} className={INPUT_CLS} />
                  {isAgentNode && (
                    <>
                      <label className={label}>agent</label>
                      <Select
                        value={node.agentId ?? ''}
                        options={agents.map((a) => ({ value: a.id, label: a.name }))}
                        onChange={(v) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, agentId: v } : n)) }))}
                        placeholder="select agent…"
                      />
                    </>
                  )}
                  {node.type === 'loop' && (
                    <>
                      <label className={label}>max iterations</label>
                      <input type="number" min={1} value={String(node.config?.maxIterations ?? 3)} onChange={(e) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, config: { ...n.config, maxIterations: Number(e.target.value) } } : n)) }))} className={INPUT_CLS} />
                    </>
                  )}
                  {node.type === 'gate' && (
                    <>
                      <label className={label}>condition</label>
                      <input value={String(node.config?.condition ?? '')} onChange={(e) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, config: { ...n.config, condition: e.target.value } } : n)) }))} placeholder='e.g. $pass == false or contains("error")' className={INPUT_CLS} />
                      <p className="mt-1.5 mb-0 text-[11px] leading-relaxed text-muted">Evaluated against the previous node's output. Supports: <code className="font-mono">$pass</code>, <code className="font-mono">$len</code>, <code className="font-mono">contains("...")</code>, <code className="font-mono">==</code>, <code className="font-mono">&gt;</code>, <code className="font-mono">&lt;</code>, dotted paths like <code className="font-mono">$result.items.length</code>.</p>
                    </>
                  )}
                  {node.type === 'checker' && (
                    <CheckerRules
                      config={node.config ?? {}}
                      update={(fn) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, config: fn(n.config ?? {}) } : n)) }))}
                    />
                  )}
                  {node.type === 'trigger' && (
                    <>
                      <label className={label}>prompt template</label>
                      <textarea value={String(node.config?.prompt ?? '')} onChange={(e) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, config: { ...n.config, prompt: e.target.value } } : n)) }))} placeholder="Optional template for the workflow input. Use {{input}} for the run input." rows={4} className={PROSE_CLS} />
                    </>
                  )}
                  {node.type === 'integration' && (
                    <>
                      <label className={label}>integration</label>
                      <Select
                        value={String(node.config?.integrationId ?? '')}
                        options={integrations.map((i) => ({ value: i.id, label: `${i.name}${i.connected ? '' : ' (not connected)'}` }))}
                        onChange={(v) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, config: { ...n.config, integrationId: v, action: '' } } : n)) }))}
                        placeholder="select integration…"
                      />
                      {node.config?.integrationId && (() => {
                        const integ = integrations.find((i) => i.id === node.config?.integrationId);
                        return (
                          <>
                            <label className={label}>action</label>
                            <Select
                              value={String(node.config?.action ?? '')}
                              options={(integ?.actions ?? []).map((a) => ({ value: a.name, label: a.name }))}
                              onChange={(v) => update((w) => ({ ...w, nodes: w.nodes.map((n) => (n.id === node.id ? { ...n, config: { ...n.config, action: v } } : n)) }))}
                              placeholder="select action…"
                            />
                          </>
                        );
                      })()}
                      <p className="mt-2 mb-0 text-[11px] leading-relaxed text-muted">The workflow's accumulated output is sent as the action's payload (content/title) where applicable.</p>
                    </>
                  )}
                  <p className="mt-4 mb-0 text-[11px] leading-relaxed text-muted">{TYPE_META[node.type].label} node. Drag its port to connect output to another node.</p>
                </div>
              );
            })() : (
              <div className="text-center text-muted">
                <p className="m-0 text-[12px] leading-relaxed">Select a node to edit its config, or drag from a node's port to create a connection.</p>
              </div>
            )}
            {storedRuns.length > 0 && (
              <div className="mt-4 border-t border-border pt-3">
                <span className={`${GROUP_LABEL_CLS} mb-2 block`}>past runs</span>
                <div className="grid gap-1.5">
                  {storedRuns.map((r) => (
                    <button
                      key={r.id}
                      type="button"
                      onClick={() => showStoredRun(r)}
                      className="focus-ring flex cursor-pointer items-center gap-2 rounded-lg border border-border bg-background px-2.5 py-2 text-left transition-colors hover:border-foreground/30"
                    >
                      <span className={`shrink-0 ${statusClass(r.status)}`}><StatusGlyph status={r.status} /></span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-mono text-[10px] text-muted">{fmtWhen(r.startedAt)}</span>
                        <span className="block truncate font-mono text-[10px] text-muted">{r.input || 'no input'}</span>
                      </span>
                      <span className="shrink-0 font-mono text-[10px] text-muted">{(r.promptTokens + r.completionTokens).toLocaleString()} tok</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ---------------- Workflow list screen ----------------
  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* No view title — the header row carries only search and the action. */}
      <div className="mb-4 flex shrink-0 flex-wrap items-center justify-end gap-2">
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="search workflows…"
          className={`${INPUT_CLS} w-52`}
        />
        <Button variant="primary" icon={<Plus size={13} />} onClick={createNew}>new workflow</Button>
      </div>

      {(() => {
        const q = search.trim().toLowerCase();
        const filtered = q ? workflows.filter((w) => w.name.toLowerCase().includes(q)) : workflows;
        if (workflows.length === 0) {
          return (
            <div className="grid flex-1 place-items-center rounded-xl border border-dashed border-border">
              <div className="text-center text-muted">
                <WorkflowIcon size={26} className="mx-auto mb-2 opacity-50" />
                <h3 className="mb-1 font-mono text-xs lowercase text-foreground">no workflows yet</h3>
                <p className="m-0 text-[12px]">Create your first workflow to chain agents and tools.</p>
              </div>
            </div>
          );
        }
        if (filtered.length === 0) {
          return (
            <div className="grid flex-1 place-items-center rounded-xl border border-dashed border-border">
              <p className="m-0 text-[12px] text-muted">No workflows match "{search}".</p>
            </div>
          );
        }
        return (
          <div className="grid grid-cols-1 gap-2.5 md:grid-cols-2 xl:grid-cols-3">
            {filtered.map((w) => (
              <Card key={w.id} className="group flex flex-col gap-3 hover:bg-surface">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex min-w-0 items-start gap-3">
                    <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-muted"><WorkflowIcon size={15} /></span>
                    <div className="min-w-0">
                      <b className="block truncate font-mono text-[11px] text-foreground">{w.name}</b>
                      <span className="mt-0.5 block font-mono text-[10px] text-muted">{w.nodes.length} nodes · {w.edges.length} connections</span>
                    </div>
                  </div>
                  <span className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100">
                    <IconButton label="delete workflow" onClick={() => openConfirm(w)}><Trash2 size={12} /></IconButton>
                  </span>
                </div>
                <span className="block font-mono text-[10px] text-muted">updated {fmtDate(w.updatedAt)}</span>
                <div className="flex justify-end">
                  <Button onClick={() => openWorkflow(w)}>open builder</Button>
                </div>
              </Card>
            ))}
          </div>
        );
      })()}

      {confirmTarget && (
        <DeleteConfirm
          name={confirmTarget.name}
          description="This permanently removes the workflow and its connections. Type the workflow's name to confirm."
          onCancel={closeConfirm}
          onConfirm={() => { deleteWorkflow(confirmTarget.id); closeConfirm(); }}
        />
      )}
    </div>
  );
}
