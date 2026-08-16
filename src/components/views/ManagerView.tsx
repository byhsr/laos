import { useEffect, useRef, useState } from 'react';
import { Bot, CornerDownLeft, Send } from 'lucide-react';
import type { Agent, Integration, ModelConfig, Task } from '../../types';
import { useManagerStore } from '../../hooks/useManager';
import { useTasksStore } from '../../hooks/useTasks';
import { StreamIndicator } from '../ui/StreamIndicator';

const STATUS_COLOR: Record<string, string> = {
  pending: 'text-[#facc15]',
  running: 'text-[#38bdf8]',
  completed: 'text-[#22c55e]',
  failed: 'text-[#f87171]',
  cancelled: 'text-muted',
};

export function ManagerView({ agents, integrations, models }: { agents: Agent[]; integrations: Integration[]; models: ModelConfig[] }) {
  const messages = useManagerStore((s) => s.messages);
  const busy = useManagerStore((s) => s.busy);
  const currentAgentId = useManagerStore((s) => s.currentAgentId);
  const setCurrentAgent = useManagerStore((s) => s.setCurrentAgent);
  const send = useManagerStore((s) => s.send);
  const loadHistory = useManagerStore((s) => s.loadHistory);
  const tasks = useTasksStore((s) => s.tasks);
  const loadTasks = useTasksStore((s) => s.loadTasks);
  const [input, setInput] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => { loadTasks(); }, [loadTasks]);
  useEffect(() => {
    const managerId = agents.find((a) => a.isManager)?.id ?? 'manager';
    loadHistory(managerId);
  }, [agents, loadHistory]);
  useEffect(() => { scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' }); }, [messages]);

  const managerAgent = agents.find((a) => a.isManager) ?? {
    id: 'manager', name: 'Manager', objective: '', model: models.find((m) => m.enabled)?.id ?? '', toolIds: [],
    integrations: [], memory: true, permissions: ['network'], homePath: 'agents/manager', color: '#22c55e', x: 0, y: 0, isManager: true,
  };

  const submit = async () => {
    if (!input.trim() || busy) return;
    const text = input.trim();
    setInput('');
    // /switch <agent> handled client-side for instant switching.
    const m = text.match(/^\/switch\s+(.+)$/i);
    if (m) {
      const name = m[1].trim().toLowerCase();
      const agent = agents.find((a) => a.name.toLowerCase().includes(name));
      if (agent) {
        setCurrentAgent(agent.id);
        const reply = `Switched to **${agent.name}**.`;
        // Echo as assistant message into that conversation.
        useManagerStore.setState((s) => {
          const conv = { ...s.conversations, [agent.id]: [...(s.conversations[agent.id] ?? []), { role: 'assistant' as const, content: reply, time: new Date().toLocaleTimeString() }] };
          return { conversations: conv, messages: conv[agent.id] };
        });
      } else {
        await send(text, managerAgent);
      }
      return;
    }
    await send(text, managerAgent);
  };

  const activeTasks = tasks.filter((t) => t.status === 'pending' || t.status === 'running');

  return (
    <div className="flex h-full gap-4">
      {/* Chat */}
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-[10px] border border-line bg-panel">
        <div className="flex h-[52px] flex-none items-center justify-between border-b border-line px-4">
          <div className="flex items-center gap-2">
            <Bot size={15} className="text-[var(--green)]" />
            <b className="text-[13px]">Manager</b>
            <span className="font-mono text-[10px] text-muted">{currentAgentId ? `→ ${agents.find((a) => a.id === currentAgentId)?.name ?? currentAgentId}` : 'orchestrator'}</span>
          </div>
          <button className="secondary" onClick={() => setCurrentAgent(null)}>Reset to Manager</button>
        </div>

        <div ref={scrollRef} className="chat-log flex-1 overflow-y-auto p-4">
          {messages.length === 0 && (
            <div className="m-auto max-w-[380px] text-center text-[13px] leading-1.6 text-muted">
              <p>Ask the Manager anything about your workspace — what agents exist, what they can do, or to delegate a task.</p>
              <p className="mt-2 font-mono text-[11px]">Try: "What agents do I have?" or "Ask the research agent to find competitors."</p>
            </div>
          )}
          {messages.map((m, i) => (
            <div key={i} className={`mb-3 flex ${m.role === 'user' ? 'justify-end' : 'justify-start'}`}>
              <div className={`max-w-[78%] rounded-[10px] px-3.5 py-2.5 text-[13px] leading-1.6 whitespace-pre-wrap break-words ${m.role === 'user' ? 'rounded-tr-[3px] bg-line text-text' : 'rounded-tl-[3px] border border-line bg-panel2'}`}>
                {m.content}
                {busy && i === messages.length - 1 && m.role === 'assistant' && (
                  m.content ? <span className="ml-0.5 inline-block h-3.5 w-[2px] animate-pulse bg-muted align-middle" /> : <StreamIndicator streaming />
                )}
              </div>
            </div>
          ))}
        </div>

        <div className="flex flex-none items-end gap-2.5 border-t border-line p-3">
          <textarea
            rows={2}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); submit(); } }}
            placeholder="Message the Manager…  (/switch &lt;agent&gt;)"
            className="flex-1 resize-none rounded-lg border border-line bg-panel2 px-3 py-2.5 text-[13px] text-text outline-none placeholder:text-muted focus:border-mid"
          />
          <button className="primary" onClick={submit} disabled={busy || !input.trim()}><Send size={13} /></button>
        </div>
      </div>

      {/* Workspace panel */}
      <div className="w-[260px] shrink-0 overflow-y-auto rounded-[10px] border border-line bg-panel p-3.5">
        <span className="mb-2 block font-mono text-[10px] tracking-[1px] text-muted">AGENTS</span>
        <div className="mb-4 grid gap-1">
          {agents.filter((a) => !a.isManager).map((a) => (
            <button
              key={a.id}
              className={`cursor-pointer rounded-[6px] border-0 px-2.5 py-2 text-left text-[12px] ${currentAgentId === a.id ? 'bg-panel2 text-text' : 'text-muted hover:bg-line'}`}
              onClick={() => setCurrentAgent(a.id)}
            >
              <span className="mr-1.5 inline-block h-2 w-2 rounded-full" style={{ background: a.color }} />
              {a.name}
              <span className="block font-mono text-[9px] text-muted">{a.integrations.join(', ') || 'no integrations'}</span>
            </button>
          ))}
          {agents.filter((a) => !a.isManager).length === 0 && <p className="px-2 text-[11px] text-muted">No agents yet.</p>}
        </div>

        <span className="mb-2 block font-mono text-[10px] tracking-[1px] text-muted">INTEGRATIONS</span>
        <div className="mb-4 grid gap-1">
          {integrations.map((i) => (
            <div key={i.id} className="flex items-center gap-2 rounded-[6px] px-2.5 py-1.5 text-[11px]">
              <i className={`inline-block h-1.5 w-1.5 rounded-full ${i.connected ? 'bg-[var(--green)]' : 'bg-[#f79009]'}`} />
              <span className="text-muted">{i.name}</span>
            </div>
          ))}
        </div>

        <span className="mb-2 block font-mono text-[10px] tracking-[1px] text-muted">ACTIVE TASKS</span>
        <div className="grid gap-1.5">
          {activeTasks.length === 0 && <p className="px-2 text-[11px] text-muted">No active tasks.</p>}
          {activeTasks.map((t: Task) => (
            <div key={t.id} className="rounded-[6px] border border-line bg-panel2 p-2">
              <div className="flex items-center justify-between">
                <b className="text-[11px]">{agents.find((a) => a.id === t.assignedAgent)?.name ?? t.assignedAgent}</b>
                <span className={`font-mono text-[9px] ${STATUS_COLOR[t.status]}`}>{t.status}</span>
              </div>
              <p className="mt-1 line-clamp-2 text-[10.5px] leading-1.5 text-muted">{t.input}</p>
            </div>
          ))}
        </div>

        <span className="mb-2 mt-4 block font-mono text-[10px] tracking-[1px] text-muted">COMMANDS</span>
        <div className="grid gap-1 text-[11px] text-muted">
          <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[10px]">/switch &lt;agent&gt;</span>
          <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[10px]">/agents</span>
          <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[10px]">/tasks</span>
          <CornerDownLeft size={12} className="mt-1 opacity-50" />
        </div>
      </div>
    </div>
  );
}
