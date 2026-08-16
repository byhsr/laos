import { useEffect, useRef, useState } from 'react';
import { Bot, CornerDownLeft, Info, Send, Trash2 } from 'lucide-react';
import type { Agent, Integration, ModelConfig, Task } from '../../types';
import { useManagerStore, type ChatEntry } from '../../hooks/useManager';
import { useTasksStore } from '../../hooks/useTasks';
import { useAgentsStore } from '../../hooks/useAgents';
import { StreamIndicator } from '../ui/StreamIndicator';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { Drawer } from '../ui/Drawer';
import { Dropdown } from '../ui/Dropdown';
import { AgentAvatar, PersonaPicker } from '../ui/AgentAvatar';
import { toast } from '../../hooks/useToast';
import { deleteChatSession, getChatSession, listChatSessions } from '../../runtime';

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
  const reset = useManagerStore((s) => s.reset);
  const newSession = useManagerStore((s) => s.newSession);
  const tasks = useTasksStore((s) => s.tasks);
  const loadTasks = useTasksStore((s) => s.loadTasks);
  const [input, setInput] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [configOpen, setConfigOpen] = useState(false);
  const [mgrDraft, setMgrDraft] = useState<Agent | null>(null);
  const [sessions, setSessions] = useState<{ id: string; title: string; createdAt: string; updatedAt: string }[]>([]);
  const [viewingSession, setViewingSession] = useState<string | null>(null);
  const [viewMsgs, setViewMsgs] = useState<ChatEntry[]>([]);
  const sessionId = useManagerStore((s) => s.sessionId);
  const persistAgent = useAgentsStore((s) => s.persistAgent);

  const openConfig = () => {
    const m = agents.find((a) => a.isManager) ?? managerAgent;
    setMgrDraft({ ...m });
    setConfigOpen(true);
  };

  const saveConfig = async () => {
    if (!mgrDraft) return;
    await persistAgent(mgrDraft);
    toast('Laos config saved', 'success');
    setConfigOpen(false);
  };

  useEffect(() => { loadTasks(); }, [loadTasks]);
  useEffect(() => {
    const managerId = agents.find((a) => a.isManager)?.id ?? 'manager';
    loadHistory(managerId);
    listChatSessions(managerId).then(setSessions).catch(() => {});
  }, [agents, loadHistory, sessionId, messages.length]);
  useEffect(() => { scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' }); }, [messages]);

  const managerAgent = agents.find((a) => a.isManager) ?? {
    id: 'manager', name: 'Manager', objective: '', model: models.find((m) => m.enabled)?.id ?? '', toolIds: [],
    integrations: [], memory: true, permissions: ['network'], homePath: 'agents/manager', color: '#22c55e', x: 0, y: 0, isManager: true,
  };

  // Deterministic command handler — these never reach the LLM.
  const runCommand = (cmd: string, text: string): string | null => {
    const arg = (re: RegExp) => { const m = text.match(re); return m ? m[1].trim() : null; };

    if (cmd === '/agents') {
      const list = agents.filter((a) => !a.isManager);
      if (list.length === 0) return 'No agents yet. Create one with "create an agent…".';
      return list.map((a) => `- **${a.name}** (${a.model})\n  ${a.objective || 'no objective'}`).join('\n');
    }
    if (cmd === '/tasks') {
      if (tasks.length === 0) return 'No tasks yet.';
      return tasks.map((t) => `- **${t.status}** → ${agents.find((a) => a.id === t.assignedAgent)?.name ?? t.assignedAgent}: ${t.input}`).join('\n');
    }
    if (cmd === '/help') {
      return 'Available commands:\n- /agents — list agents\n- /tasks — list tasks\n- /switch &lt;agent&gt; — switch conversation to an agent\n- /help — this message\n\nEverything else goes to Laos.';
    }
    if (cmd === '/switch') {
      const name = arg(/^\/switch\s+(.+)$/i);
      if (!name) return 'Usage: /switch &lt;agent name&gt;';
      const agent = agents.find((a) => !a.isManager && a.name.toLowerCase().includes(name.toLowerCase()));
      if (!agent) return `No agent named "${name}". Try /agents to list them.`;
      setCurrentAgent(agent.id);
      return `Switched to **${agent.name}**.`;
    }
    return null;
  };

  const echo = (agentId: string, reply: string) => {
    useManagerStore.setState((s) => {
      const conv = { ...s.conversations, [agentId]: [...(s.conversations[agentId] ?? []), { role: 'assistant' as const, content: reply, time: new Date().toLocaleTimeString() }] };
      return { conversations: conv, messages: conv[agentId] };
    });
  };

  // Auto-grow the input up to a max height as the user types.
  const onInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    e.target.style.height = 'auto';
    e.target.style.height = `${Math.min(e.target.scrollHeight, 160)}px`;
  };

  const submit = async () => {
    if (!input.trim() || busy) return;
    const text = input.trim();
    setInput('');
    if (inputRef.current) inputRef.current.style.height = 'auto';
    // Deterministic slash commands — resolved locally, no LLM involved.
    const cmd = text.toLowerCase().split(/\s+/)[0];
    const reply = runCommand(cmd, text);
    if (reply !== null) {
      const agentId = useManagerStore.getState().currentAgentId ?? managerAgent.id;
      echo(agentId, reply);
      return;
    }
    await send(text, managerAgent);
  };

  const activeTasks = tasks.filter((t) => t.status === 'pending' || t.status === 'running');
  const [infoOpen, setInfoOpen] = useState(false);

  return (
    <div className="flex h-full flex-col gap-4">
      {/* Tab bar */}
      <header className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          <Bot size={14} className="text-[var(--green)]" />
          <span className="font-mono text-[10px] uppercase tracking-[1px] text-text">Laos</span>
          <span className="font-mono text-[10px] text-muted">{currentAgentId ? `→ ${agents.find((a) => a.id === currentAgentId)?.name ?? currentAgentId}` : ''}</span>
        </div>
        <div className="flex items-center gap-1">
          <button className={`flex cursor-pointer items-center gap-1 rounded-[6px] border px-2.5 py-1.5 text-[11px] capitalize ${!infoOpen ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`} onClick={() => setInfoOpen(false)}>Chat</button>
          <button className={`flex cursor-pointer items-center gap-1 rounded-[6px] border px-2.5 py-1.5 text-[11px] capitalize ${infoOpen ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`} onClick={() => setInfoOpen(true)}>
            Info
            <span className="group relative inline-flex">
              <Info size={11} className="text-mid" />
              <span className="pointer-events-none absolute bottom-full left-1/2 z-50 mb-1.5 hidden -translate-x-1/2 whitespace-nowrap rounded-[5px] border border-line bg-panel2 px-2 py-1 text-[10px] font-normal text-text shadow-[0_8px_20px_#000a] group-hover:block">Agents, integrations, active tasks, chats &amp; commands</span>
            </span>
          </button>
          <button className="rounded-[6px] border-0 bg-none px-2.5 py-1.5 text-[11px] text-muted hover:bg-panel2 hover:text-text" onClick={() => { const id = agents.find((a) => a.isManager)?.id ?? 'manager'; const m = agents.find((a) => a.id === id)?.model ?? models.find((x) => x.enabled)?.id ?? ''; newSession(id, m); }}>New chat</button>
          <button className="rounded-[6px] border-0 bg-none px-2.5 py-1.5 text-[11px] text-muted hover:bg-panel2 hover:text-text" onClick={openConfig}>Config</button>
          <button className="rounded-[6px] border-0 bg-none px-2.5 py-1.5 text-[11px] text-muted hover:bg-panel2 hover:text-text" onClick={() => { const id = agents.find((a) => a.isManager)?.id ?? 'manager'; reset(id); }} title="Clear Laos's memory">Reset memory</button>
        </div>
      </header>

      {!infoOpen && (
        <div className="relative flex min-h-0 flex-1 flex-col">
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-panel">
            <div ref={scrollRef} className="chat-log flex-1 scrollbar-thin scrollbar-color-mid overflow-y-auto px-4 pt-4 pb-24">
              {messages.length === 0 && (
                <div className="m-auto max-w-[380px] text-center text-[13px] leading-1.6 text-muted">
                  <p>Ask Laos anything about your workspace — what agents exist, what they can do, or to delegate a task.</p>
                  <p className="mt-2 font-mono text-[11px]">Try: "What agents do I have?" or "Ask the research agent to find competitors."</p>
                </div>
              )}
              {messages.map((m, i) => (
                <div key={i} className={`mb-3 flex ${m.role === 'user' ? 'justify-end' : 'justify-start'} last:mb-0`}>
                  <div className={`max-w-[78%] rounded-[10px] px-3.5 py-2.5 text-[13px] leading-1.6 whitespace-pre-wrap break-words ${m.role === 'user' ? 'rounded-tr-[3px] bg-line text-text' : 'rounded-tl-[3px] border border-line bg-panel2'}`}>
                    {m.content}
                    {busy && i === messages.length - 1 && m.role === 'assistant' && (
                      m.content ? <span className="ml-0.5 inline-block h-3.5 w-[2px] animate-pulse bg-muted align-middle" /> : <StreamIndicator streaming />
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>

          <ConfirmDialog />

          {/* Input overlays the chat, floating at the bottom */}
          <div className="absolute right-0 bottom-0 left-0 flex items-end gap-2.5 rounded-lg bg-gradient-to-t from-[var(--bg)] via-[var(--bg)]/80 to-transparent p-3 pt-6">
            <textarea
              ref={inputRef}
              rows={1}
              value={input}
              onChange={onInputChange}
              onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); submit(); } }}
              placeholder="Message Laos…  (/agents, /tasks, /switch, /help)"
              style={{ flex: 1, background: 'var(--panel2)', border: '1px solid var(--line)', borderRadius: 10, padding: '12px 14px', color: 'var(--text)', resize: 'none', minHeight: 44, maxHeight: 160, boxShadow: '0 8px 24px #000a', outline: 'none' }}
            />
            <button className="primary" onClick={submit} disabled={busy || !input.trim()} title="Send"><Send size={14} /></button>
          </div>
        </div>
      )}

      {infoOpen && (
        <div className="grid max-h-[calc(100vh-220px)] grid-cols-1 gap-4 overflow-y-auto md:grid-cols-2 xl:grid-cols-3">
          {/* Agents */}
          <div className="rounded-[10px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[10px] tracking-[1px] text-muted">AGENTS</span>
            <div className="grid gap-1">
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
          </div>

          {/* Integrations */}
          <div className="rounded-[10px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[10px] tracking-[1px] text-muted">INTEGRATIONS</span>
            <div className="grid gap-1">
              {integrations.map((i) => (
                <div key={i.id} className="flex items-center gap-2 rounded-[6px] px-2.5 py-1.5 text-[11px]">
                  <i className={`inline-block h-1.5 w-1.5 rounded-full ${i.connected ? 'bg-[var(--green)]' : 'bg-[#f79009]'}`} />
                  <span className="text-muted">{i.name}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Active tasks */}
          <div className="rounded-[10px] border border-line bg-panel p-3.5">
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
          </div>

          {/* Chats */}
          <div className="rounded-[10px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[10px] tracking-[1px] text-muted">CHATS</span>
            <div className="grid gap-1.5">
              {sessions.length === 0 && <p className="px-2 text-[11px] text-muted">No chats yet. Send a message to start one.</p>}
              {sessions.map((s) => (
                <div key={s.id} className={`flex items-center gap-1 rounded-[6px] border px-2 py-1.5 ${s.id === sessionId ? 'border-[var(--green)] bg-panel2' : 'border-line bg-panel2/50'}`}>
                  <button className="flex-1 cursor-pointer overflow-hidden text-left" onClick={async () => {
                    setViewingSession(s.id);
                    const msgs = await getChatSession(s.id);
                    setViewMsgs(msgs.map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content, time: '' })));
                  }}>
                    <span className="block truncate text-[11px] text-text">{s.id === sessionId ? '● Current chat' : s.title}</span>
                    <span className="block font-mono text-[9px] text-muted">{s.updatedAt ? (() => { const d = new Date(s.updatedAt); return isNaN(d.getTime()) ? '' : d.toLocaleString(); })() : ''}</span>
                  </button>
                  <button className="cursor-pointer border-0 bg-transparent p-1 text-muted hover:text-[#f87171]" onClick={async () => { await deleteChatSession(s.id); listChatSessions(agents.find((a) => a.isManager)?.id ?? 'manager').then(setSessions); }}><Trash2 size={11} /></button>
                </div>
              ))}
            </div>
          </div>

          {/* Commands */}
          <div className="rounded-[10px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[10px] tracking-[1px] text-muted">COMMANDS</span>
            <div className="grid gap-1 text-[11px] text-muted">
              <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[10px]">/switch &lt;agent&gt;</span>
              <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[10px]">/agents</span>
              <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[10px]">/tasks</span>
              <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[10px]">/help</span>
              <CornerDownLeft size={12} className="mt-1 opacity-50" />
            </div>
          </div>
        </div>
      )}

      {configOpen && mgrDraft && (
        <Drawer
          title="Laos config"
          onClose={() => setConfigOpen(false)}
          initialWidth={Math.round(window.innerWidth / 2)}
          resizable
          headerAction={<button className="primary" onClick={saveConfig}>Save</button>}
        >
          <div className="w-full rounded-[10px] border border-line bg-panel p-[22px]">
            <label className="block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">MODEL</label>
            <Dropdown
              value={mgrDraft.model}
              options={models.map((m) => ({ value: m.id, label: m.label }))}
              onChange={(v) => setMgrDraft({ ...mgrDraft, model: v })}
            />
            {models.length === 0 && <p className="mt-2 text-[12px] text-muted">No models configured. Add one in the Models tab.</p>}

            <label className="mt-4 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">PERSONA</label>
            <PersonaPicker value={mgrDraft.persona ?? 'ai-orb'} onChange={(v) => setMgrDraft({ ...mgrDraft, persona: v })} />
            <div className="mt-2 flex items-center gap-2">
              <AgentAvatar agent={{ ...mgrDraft, persona: mgrDraft.persona ?? 'ai-orb' }} size={28} />
              <span className="text-[11px] text-muted">Animated Lottie gremlin avatar</span>
            </div>

            <label className="mt-4 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">OBJECTIVE / PROMPT</label>
            <textarea
              value={mgrDraft.objective}
              onChange={(e) => setMgrDraft({ ...mgrDraft, objective: e.target.value })}
              rows={5}
              placeholder="Describe Laos's role…"
              className="mt-1.5 w-full resize-y rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] leading-1.6 text-text outline-none focus:border-mid"
            />
            <p className="mt-1.5 text-[11px] leading-relaxed text-muted">The system prompt is generated dynamically from your workspace and Laos's tools. This objective is a seed/fallback description.</p>

            <label className="mt-4 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">PERMISSIONS</label>
            <div className="mt-1.5 flex flex-wrap gap-3">
              {(['network', 'files', 'host_fs'] as const).map((p) => (
                <label key={p} className="flex items-center gap-1.5 text-[12px]">
                  <input
                    type="checkbox"
                    checked={mgrDraft.permissions.includes(p)}
                    onChange={() => setMgrDraft({ ...mgrDraft, permissions: mgrDraft.permissions.includes(p) ? mgrDraft.permissions.filter((x) => x !== p) : [...mgrDraft.permissions, p] })}
                  />
                  {p}
                </label>
              ))}
            </div>

            <label className="mt-4 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">MEMORY</label>
            <div className="mt-1.5 flex items-center gap-2">
              <button className="secondary" onClick={() => { const id = mgrDraft.id; reset(id); toast('Laos memory cleared', 'success'); }}>Reset memory</button>
            </div>
          </div>
        </Drawer>
      )}

      {viewingSession && (
        <div className="fixed inset-0 z-[90] grid place-items-center bg-black/60" onClick={() => setViewingSession(null)}>
          <div className="flex max-h-[70vh] w-[520px] max-w-[92vw] flex-col rounded-[12px] border border-line bg-panel shadow-[0_20px_60px_#000a]" onClick={(e) => e.stopPropagation()}>
            <div className="flex h-[48px] flex-none items-center justify-between border-b border-line px-4">
              <b className="text-[13px]">Chat history</b>
              <button className="secondary" onClick={() => setViewingSession(null)}>Close</button>
            </div>
            <div className="flex-1 overflow-y-auto p-4">
              {viewMsgs.length === 0 ? <p className="text-center text-[12px] text-muted">No messages.</p> : viewMsgs.map((m, i) => (
                <div key={i} className={`mb-2 flex ${m.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                  <div className={`max-w-[80%] rounded-[10px] px-3 py-2 text-[12.5px] leading-1.6 whitespace-pre-wrap break-words ${m.role === 'user' ? 'rounded-tr-[3px] bg-line text-text' : 'rounded-tl-[3px] border border-line bg-panel2'}`}>{m.content}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
