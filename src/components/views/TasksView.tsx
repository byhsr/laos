import { useEffect, useState } from 'react';
import { Play, Trash2 } from 'lucide-react';
import type { Agent, Task } from '../../types';
import { useTasksStore } from '../../hooks/useTasks';
import { toast } from '../../hooks/useToast';
import { Select } from '../ui/Select';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import { StatusTag } from '../ui/Status';
import { FIELD_LABEL_CLS, INPUT_CLS, PROSE_CLS } from '../ui/Input';

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
    toast(`task sent to ${agent?.name ?? agentId}`, 'success');
    setInput(''); setContext('');
  };

  return (
    <>
      <Card className="mb-4 hover:bg-surface">
        <label className={FIELD_LABEL_CLS}>assign to</label>
        <div className="mb-3">
          <Select
            value={agentId}
            options={[{ value: '', label: 'select agent…' }, ...agents.filter((a) => !a.isManager).map((a) => ({ value: a.id, label: a.name }))]}
            onChange={setAgentId}
          />
        </div>
        <label className={FIELD_LABEL_CLS}>task</label>
        <textarea rows={2} value={input} onChange={(e) => setInput(e.target.value)} placeholder="What should the agent do?" className={`${PROSE_CLS} mb-3 min-h-[64px]`} />
        <label className={FIELD_LABEL_CLS}>context (optional)</label>
        <textarea rows={2} value={context} onChange={(e) => setContext(e.target.value)} placeholder="Extra context the agent should know…" className={`${PROSE_CLS} mb-3 min-h-[64px]`} />
        <Button variant="primary" icon={<Play size={13} />} onClick={submit} disabled={!agentId || !input.trim()}>run task</Button>
      </Card>

      <div className="grid gap-2">
        {tasks.length === 0 && <p className="m-0 font-mono text-[11px] text-muted">no tasks yet</p>}
        {tasks.map((t: Task) => (
          <Card key={t.id} className="hover:bg-surface">
            <div className="flex flex-wrap items-center gap-3">
              <StatusTag status={t.status} />
              <b className="min-w-0 truncate font-mono text-[11px] text-foreground">{agents.find((a) => a.id === t.assignedAgent)?.name ?? t.assignedAgent}</b>
              <span className="ml-auto font-mono text-[10px] text-muted">{(() => { const d = new Date(t.createdAt); return isNaN(d.getTime()) ? '' : d.toLocaleString(); })()}</span>
              {(t.status === 'pending' || t.status === 'running') && (
                <Button icon={<Trash2 size={12} />} onClick={() => cancelTask(t.id)}>cancel</Button>
              )}
            </div>
            <p className="mt-2 mb-0 font-mono text-[11px] text-foreground">$ {t.input}</p>
            {t.context && <p className="mt-1 mb-0 font-mono text-[10px] text-muted">{t.context}</p>}
            {t.result && <pre className="mt-2 mb-0 max-h-32 overflow-x-hidden overflow-y-auto rounded border border-border bg-background p-2 font-mono text-[10px] whitespace-pre-wrap text-muted">{t.result}</pre>}
          </Card>
        ))}
      </div>
    </>
  );
}
