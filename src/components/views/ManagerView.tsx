import { useEffect, useRef, useState } from 'react';
import { CornerDownLeft, Info, MessageSquare, MessageSquarePlus, RefreshCw, RotateCcw, Send, Settings, Trash2 } from 'lucide-react';
import { ContextMenu, type MenuItem } from '../ui/ContextMenu';
import type { Agent, Integration, ModelConfig, Task } from '../../types';
import { useManagerStore, type ChatEntry } from '../../hooks/useManager';
import { useTasksStore } from '../../hooks/useTasks';
import { useAgentsStore } from '../../hooks/useAgents';
import { useShallow } from 'zustand/react/shallow';
import { StreamIndicator } from '../ui/StreamIndicator';
import { Markdown } from '../ui/Markdown';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { Drawer } from '../ui/Drawer';
import { Dropdown } from '../ui/Dropdown';
import { AgentAvatar, PersonaPicker } from '../ui/AgentAvatar';
import { toast } from '../../hooks/useToast';
import { deleteChatSession, getChatSession, listChatSessions, listTelegramLogs, type TelegramLogEntry } from '../../runtime';

const STATUS_COLOR: Record<string, string> = {
  pending: 'text-[#facc15]',
  running: 'text-[#38bdf8]',
  completed: 'text-[#22c55e]',
  failed: 'text-[#f87171]',
  cancelled: 'text-muted',
};

export function ManagerView({ agents, integrations, models }: { agents: Agent[]; integrations: Integration[]; models: ModelConfig[] }) {
  const managerId = agents.find((a) => a.isManager)?.id ?? 'manager';
  // Read the manager's own conversation directly (like AgentWindow), so the
  // visible chat survives tab switches regardless of the shared currentAgentId.
  const messages = useManagerStore(useShallow((s) => s.conversations[managerId] ?? []));
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
    loadHistory(managerId);
    listChatSessions(managerId).then(setSessions).catch(() => {});
  }, [managerId, loadHistory]);
  useEffect(() => { scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' }); }, [messages]);

  const managerAgent = agents.find((a) => a.isManager) ?? {
    id: 'manager', name: 'Manager', objective: '', model: models.find((m) => m.enabled)?.id ?? '', toolIds: [],
    integrations: [], skillIds: [], memory: true, permissions: ['network'], homePath: 'agents/manager', color: '#22c55e', x: 0, y: 0, isManager: true,
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
  const [tab, setTab] = useState<'chat' | 'info' | 'telegram'>('chat');
  const [telegramLogs, setTelegramLogs] = useState<TelegramLogEntry[]>([]);

  const loadTelegramLogs = async () => {
    setTelegramLogs(await listTelegramLogs());
  };
  useEffect(() => {
    if (tab === 'telegram') {
      loadTelegramLogs();
      const iv = setInterval(loadTelegramLogs, 3000);
      return () => clearInterval(iv);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  const menuItems: MenuItem[] = [
    { key: 'chat', label: 'Chat', icon: <MessageSquare size={13} />, active: tab === 'chat', onSelect: () => setTab('chat') },
    { key: 'info', label: 'Info', icon: <Info size={13} />, active: tab === 'info', onSelect: () => setTab('info') },
    { key: 'telegram', label: 'Telegram', icon: <Send size={13} />, active: tab === 'telegram', onSelect: () => setTab('telegram') },
    { key: 'new', label: 'New chat', icon: <MessageSquarePlus size={13} />, dividerBefore: true, onSelect: () => {
      const m = agents.find((a) => a.id === managerId)?.model ?? models.find((x) => x.enabled)?.id ?? '';
      void newSession(managerId, m);
    } },
    { key: 'config', label: 'Config', icon: <Settings size={13} />, onSelect: openConfig },
    { key: 'reset', label: 'Reset memory', icon: <RotateCcw size={13} />, onSelect: () => { void reset(managerId); } },
  ];

  return (
    <div className="flex h-full flex-col gap-1">
      <header className="flex shrink-0 items-center justify-end">
        <ContextMenu items={menuItems} />
      </header>

      {tab === 'chat' && (
        <div className="relative flex min-h-0 flex-1 flex-col">
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-panel">
            <div ref={scrollRef} className="chat-log flex-1 scrollbar-thin scrollbar-color-mid overflow-y-auto px-4 pt-4 pb-24">
              {messages.map((m, i) => (
                <div key={i} className={`mb-3 flex ${m.role === 'user' ? 'justify-end' : 'justify-start'} last:mb-0`}>
                  <div className={`max-w-[78%] rounded-[16px] px-3.5 py-2.5 text-[13px] leading-1.6 break-words ${m.role === 'user' ? 'whitespace-pre-wrap rounded-tr-[8px] bg-line text-text' : 'rounded-tl-[8px] border border-line bg-panel2'}`}>
                    {m.role === 'user' ? m.content : <Markdown>{m.content}</Markdown>}
                    {busy && i === messages.length - 1 && m.role === 'assistant' && (
                      m.content ? <span className="ml-0.5 inline-block h-3.5 w-[2px] animate-pulse bg-muted align-middle" /> : <StreamIndicator streaming />
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Input overlays the chat, floating at the bottom */}
          <div className="absolute right-0 bottom-0 left-0 flex items-end gap-3 rounded-b-[16px] bg-gradient-to-t from-[var(--panel)] via-[var(--panel)]/85 to-transparent p-3 pt-6">
            <div className="absolute right-3 bottom-[calc(100%+8px)] left-3 z-[30]">
              <ConfirmDialog />
            </div>
            <textarea
              ref={inputRef}
              rows={1}
              value={input}
              onChange={onInputChange}
              onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); submit(); } }}
              placeholder="Message Laos…  (/agents, /tasks, /switch, /help)"
              style={{ flex: 1, background: 'var(--panel2)', border: '1px solid var(--color-hairline)', borderRadius: 16, padding: '12px 16px', color: 'var(--text)', resize: 'none', minHeight: 44, maxHeight: 160, boxShadow: 'var(--shadow-soft)', outline: 'none' }}
            />
            <button className="primary" onClick={submit} disabled={busy || !input.trim()} title="Send"><Send size={14} /></button>
          </div>
        </div>
      )}

      {tab === 'info' && (
        <div className="grid max-h-[calc(100vh-220px)] grid-cols-1 gap-4 overflow-y-auto md:grid-cols-2 xl:grid-cols-3">
          {/* Agents */}
          <div className="rounded-[16px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[11px] tracking-[1px] text-muted">AGENTS</span>
            <div className="grid gap-1">
              {agents.filter((a) => !a.isManager).map((a) => (
                <button
                  key={a.id}
                  className={`cursor-pointer rounded-[10px] border-0 px-2.5 py-2 text-left text-[12px] ${currentAgentId === a.id ? 'bg-panel2 text-text' : 'text-muted hover:bg-line'}`}
                  onClick={() => setCurrentAgent(a.id)}
                >
                  <span className="mr-1.5 inline-block h-2 w-2 rounded-full" style={{ background: a.color }} />
                  {a.name}
                  <span className="block font-mono text-[10px] text-muted">{a.integrations.join(', ') || 'no integrations'}</span>
                </button>
              ))}
              {agents.filter((a) => !a.isManager).length === 0 && <p className="px-2 text-[11px] text-muted">No agents yet.</p>}
            </div>
          </div>

          {/* Integrations */}
          <div className="rounded-[16px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[11px] tracking-[1px] text-muted">INTEGRATIONS</span>
            <div className="grid gap-1">
              {integrations.map((i) => (
                <div key={i.id} className="flex items-center gap-2 rounded-[10px] px-2.5 py-1.5 text-[11px]">
                  <i className={`inline-block h-1.5 w-1.5 rounded-full ${i.connected ? 'bg-[var(--green)]' : 'bg-[#f79009]'}`} />
                  <span className="text-muted">{i.name}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Active tasks */}
          <div className="rounded-[16px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[11px] tracking-[1px] text-muted">ACTIVE TASKS</span>
            <div className="grid gap-1.5">
              {activeTasks.length === 0 && <p className="px-2 text-[11px] text-muted">No active tasks.</p>}
              {activeTasks.map((t: Task) => (
                <div key={t.id} className="rounded-[10px] border border-line bg-panel2 p-2">
                  <div className="flex items-center justify-between">
                    <b className="text-[11px]">{agents.find((a) => a.id === t.assignedAgent)?.name ?? t.assignedAgent}</b>
                    <span className={`font-mono text-[10px] ${STATUS_COLOR[t.status]}`}>{t.status}</span>
                  </div>
                  <p className="mt-1 line-clamp-2 text-[10.5px] leading-1.5 text-muted">{t.input}</p>
                </div>
              ))}
            </div>
          </div>

          {/* Chats */}
          <div className="rounded-[16px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[11px] tracking-[1px] text-muted">CHATS</span>
            <div className="grid gap-1.5">
              {sessions.length === 0 && <p className="px-2 text-[11px] text-muted">No chats yet.</p>}
              {sessions.map((s) => (
                <div key={s.id} className={`flex items-center gap-1 rounded-[10px] border px-2 py-1.5 ${s.id === sessionId ? 'border-[var(--green)] bg-panel2' : 'border-line bg-panel2/50'}`}>
                  <button className="flex-1 cursor-pointer overflow-hidden text-left" onClick={async () => {
                    setViewingSession(s.id);
                    const msgs = await getChatSession(s.id);
                    setViewMsgs(msgs.map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content, time: '' })));
                  }}>
                    <span className="block truncate text-[11px] text-text">{s.id === sessionId ? '● Current chat' : s.title}</span>
                    <span className="block font-mono text-[10px] text-muted">{s.updatedAt ? (() => { const d = new Date(s.updatedAt); return isNaN(d.getTime()) ? '' : d.toLocaleString(); })() : ''}</span>
                  </button>
                  <button className="cursor-pointer border-0 bg-transparent p-1 text-muted hover:text-[#f87171]" onClick={async () => { await deleteChatSession(s.id); listChatSessions(agents.find((a) => a.isManager)?.id ?? 'manager').then(setSessions); }}><Trash2 size={11} /></button>
                </div>
              ))}
            </div>
          </div>

          {/* Commands */}
          <div className="rounded-[16px] border border-line bg-panel p-3.5">
            <span className="mb-2 block font-mono text-[11px] tracking-[1px] text-muted">COMMANDS</span>
            <div className="grid gap-1 text-[11px] text-muted">
              <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[11px]">/switch &lt;agent&gt;</span>
              <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[11px]">/agents</span>
              <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[11px]">/tasks</span>
              <span className="rounded bg-panel2 px-2 py-1.5 font-mono text-[11px]">/help</span>
              <CornerDownLeft size={12} className="mt-1 opacity-50" />
            </div>
          </div>
        </div>
      )}

      {tab === 'telegram' && (
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="mb-2 flex items-center justify-between">
            <span className="font-mono text-[11px] tracking-[1px] text-muted">TELEGRAM ACTIVITY</span>
            <button className="secondary" onClick={loadTelegramLogs}><RefreshCw size={12} />Refresh</button>
          </div>
          <div className="runs-console flex-1 overflow-y-auto rounded-lg border border-line bg-inset p-3.5 font-mono text-[12px] leading-[1.6]">
            {telegramLogs.length === 0 ? null : telegramLogs.map((l, i) => (
              <div key={i} className="border-b border-[#1c1c1f] py-2 last:border-0">
                <div className="flex items-center gap-3 text-[11px]">
                  <span className={l.direction === 'in' ? 'text-[#38bdf8]' : 'text-[#22c55e]'}>{l.direction === 'in' ? '▸ IN' : '◂ OUT'}</span>
                  <span className={`font-mono text-[11px] ${l.status === 'error' ? 'text-[#f87171]' : l.status === 'sent' ? 'text-[#22c55e]' : 'text-[#facc15]'}`}>{l.status}</span>
                  <span className="ml-auto text-mid">{l.createdAt ? (() => { const d = new Date(l.createdAt); return isNaN(d.getTime()) ? '' : d.toLocaleTimeString(); })() : ''}</span>
                </div>
                <div className="mt-1 text-[#d4d4d8]">in: {l.text}</div>
                {l.reply && <div className="mt-0.5 text-[#a1a1aa]">out: {l.reply.length > 300 ? `${l.reply.slice(0, 300)}…` : l.reply}</div>}
                {l.detail && <div className="mt-0.5 text-[#f87171]">detail: {l.detail}</div>}
              </div>
            ))}
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
          <div className="w-full rounded-[16px] border border-line bg-panel p-[22px]">
            <label className="block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">MODEL</label>
            <Dropdown
              value={mgrDraft.model}
              options={models.map((m) => ({ value: m.id, label: m.label }))}
              onChange={(v) => setMgrDraft({ ...mgrDraft, model: v })}
            />

            <label className="mt-4 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">PERSONA</label>
            <PersonaPicker value={mgrDraft.persona ?? 'ai-orb'} onChange={(v) => setMgrDraft({ ...mgrDraft, persona: v })} />
            <div className="mt-2 flex items-center gap-2">
              <AgentAvatar agent={{ ...mgrDraft, persona: mgrDraft.persona ?? 'ai-orb' }} size={28} />
            </div>

            <label className="mt-4 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">OBJECTIVE / PROMPT</label>
            <textarea
              value={mgrDraft.objective}
              onChange={(e) => setMgrDraft({ ...mgrDraft, objective: e.target.value })}
              rows={5}
              placeholder="Describe Laos's role…"
              className="mt-1.5 w-full resize-y rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] leading-1.6 text-text outline-none focus:border-mid"
            />

            <label className="mt-4 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">PERMISSIONS</label>
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

            <label className="mt-4 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">MEMORY</label>
            <div className="mt-1.5 flex items-center gap-2">
              <button className="secondary" onClick={() => { const id = mgrDraft.id; reset(id); toast('Laos memory cleared', 'success'); }}>Reset memory</button>
            </div>
          </div>
        </Drawer>
      )}

      {viewingSession && (
        <div className="fixed inset-0 z-[90] grid place-items-center bg-black/60" onClick={() => setViewingSession(null)}>
          <div className="flex max-h-[70vh] w-[520px] max-w-[92vw] flex-col rounded-[16px] border border-line bg-panel shadow-[0_20px_60px_#000a]" onClick={(e) => e.stopPropagation()}>
            <div className="flex h-[48px] flex-none items-center justify-between border-b border-line px-4">
              <b className="text-[13px]">Chat history</b>
              <button className="secondary" onClick={() => setViewingSession(null)}>Close</button>
            </div>
            <div className="flex-1 overflow-y-auto p-4">
              {viewMsgs.length === 0 ? <p className="text-center text-[12px] text-muted">No messages.</p> : viewMsgs.map((m, i) => (
                <div key={i} className={`mb-2 flex ${m.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                  <div className={`max-w-[80%] rounded-[16px] px-3 py-2 text-[12.5px] leading-1.6 break-words ${m.role === 'user' ? 'whitespace-pre-wrap rounded-tr-[8px] bg-line text-text' : 'rounded-tl-[8px] border border-line bg-panel2'}`}>{m.role === 'user' ? m.content : <Markdown>{m.content}</Markdown>}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
