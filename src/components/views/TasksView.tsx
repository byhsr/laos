import { useEffect, useState } from 'react';
import { Play, Trash2 } from 'lucide-react';
import type { Agent, Task } from '../../types';
import { useTasksStore } from '../../hooks/useTasks';
import { toast } from '../../hooks/useToast';

const STATUS_COLOR: Record<string, string> = {
  pending: 'text-[#facc15]',
  running: 'text-[#38bdf8]',
  completed: 'text-[#22c55e]',
  failed: 'text-[#f87171]',
  cancelled: 'text-muted',
};

export function TasksView({ agents }: { agents: Agent[] }) {
  const tasks = useTasksStore((s) => s.tasks);
  const loadTasks = useTasksStore((s) => s.loadTasks);
  const runTask = useTasksStore((s) => s.runTask);
  const cancelTask = useTasksStore((s) => s.cancelTask);
  const [agentId, setAgentId] = useState('');
  const [input, setInput] = useState('');
  const [context, setContext] = useState('');

  useEffect(() => { loadTasks(); }, [loadTasks]);

  const submit = async () => {
    if (!agentId || !input.trim()) return;
    const agent = agents.find((a) => a.id === agentId);
    await runTask('ui', agentId, input.trim(), context.trim());
    toast(`Task sent to ${agent?.name ?? agentId}`, 'success');
    setInput(''); setContext('');
  };

  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        <div><span className="font-mono text-[11px] tracking-[1px] text-muted">WORKSPACE</span><h1 style={{ margin: 0, fontSize: 24 }}>Tasks</h1></div>
      </header>

      <div className="mb-5 max-w-[900px] rounded-[16px] border border-line bg-panel p-[22px]">
        <label className="mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">Assign to</label>
        <select value={agentId} onChange={(e) => setAgentId(e.target.value)} className="mb-3 w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid">
          <option value="">Select agent…</option>
          {agents.filter((a) => !a.isManager).map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
        </select>
        <label className="mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">Task</label>
        <textarea rows={2} value={input} onChange={(e) => setInput(e.target.value)} placeholder="What should the agent do?" className="mb-3 w-full resize-none rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none placeholder:text-muted focus:border-mid" />
        <label className="mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">Context (optional)</label>
        <textarea rows={2} value={context} onChange={(e) => setContext(e.target.value)} placeholder="Extra context the agent should know…" className="mb-3 w-full resize-none rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none placeholder:text-muted focus:border-mid" />
        <button className="primary" onClick={submit} disabled={!agentId || !input.trim()}><Play size={13} />Run task</button>
      </div>

      <div className="grid max-w-[900px] gap-3">
        {tasks.length === 0 && <p className="text-[12px] text-muted">No tasks yet.</p>}
        {tasks.map((t: Task) => (
          <div key={t.id} className="rounded-[16px] border border-line bg-panel p-[16px]">
            <div className="flex items-center gap-3">
              <span className={`font-mono text-[11px] ${STATUS_COLOR[t.status]}`}>{t.status}</span>
              <b className="text-[12.5px]">{agents.find((a) => a.id === t.assignedAgent)?.name ?? t.assignedAgent}</b>
              <span className="ml-auto text-[11px] text-mid">{(() => { const d = new Date(t.createdAt); return isNaN(d.getTime()) ? '' : d.toLocaleString(); })()}</span>
              {(t.status === 'pending' || t.status === 'running') && (
                <button className="secondary" onClick={() => cancelTask(t.id)}><Trash2 size={12} />Cancel</button>
              )}
            </div>
            <p className="mt-1.5 text-[12px] text-text">$ {t.input}</p>
            {t.context && <p className="mt-1 text-[11px] text-muted">{t.context}</p>}
            {t.result && <pre className="mt-2 max-h-32 overflow-y-auto whitespace-pre-wrap rounded-[10px] border border-line bg-panel2 p-2 text-[11px] text-muted">{t.result}</pre>}
          </div>
        ))}
      </div>
    </>
  );
}
