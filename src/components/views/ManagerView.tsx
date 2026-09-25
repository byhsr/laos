import { useEffect, useRef, useState } from 'react';
import { CornerDownLeft, Info, MessageSquare, MessageSquarePlus, RotateCcw, Settings, Trash2 } from 'lucide-react';
import { ContextMenu, type MenuItem } from '../ui/ContextMenu';
import type { Agent, Integration, ModelConfig, Task } from '../../types';
import { useManagerStore, type ChatEntry } from '../../hooks/useManager';
import { useTasksStore } from '../../hooks/useTasks';
import { useAgentsStore } from '../../hooks/useAgents';
import { useShallow } from 'zustand/react/shallow';
import { Modal } from '../ui/Modal';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { AgentAvatar, PersonaPicker } from '../ui/AgentAvatar';
import { Checkbox } from '../ui/Checkbox';
import { StatusTag } from '../ui/Status';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS, PROSE_CLS } from '../ui/Input';
import { ChatComposer } from '../chat/ChatComposer';
import { useStickToBottom } from '../../hooks/useStickToBottom';
import { MessageBubble } from '../chat/MessageBubble';
import { toast } from '../../hooks/useToast';
import { deleteChatSession, getChatSession, listChatSessions } from '../../runtime';

const fieldLabel = `${FIELD_LABEL_CLS} mt-4`;

export function ManagerView({ agents, integrations, models, openConfigRequest = 0 }: { agents: Agent[]; integrations: Integration[]; models: ModelConfig[]; openConfigRequest?: number }) {
  const managerId = agents.find((a) => a.isManager)?.id ?? 'manager';
  // The lead agent's configured name is the only name shown anywhere — nothing
  // user-facing hardcodes it.
  const leadName = agents.find((a) => a.isManager)?.name?.trim() || 'Manager';
  // Read the manager's own conversation directly (like AgentWindow), so the
  // visible chat survives tab switches regardless of the shared currentAgentId.
  const messages = useManagerStore(useShallow((s) => s.conversations[managerId] ?? []));
  const busy = useManagerStore((s) => s.busy);
  const steps = useManagerStore((s) => s.steps);
  const currentAgentId = useManagerStore((s) => s.currentAgentId);
  const setCurrentAgent = useManagerStore((s) => s.setCurrentAgent);
  const send = useManagerStore((s) => s.send);
  const loadHistory = useManagerStore((s) => s.loadHistory);
  const reset = useManagerStore((s) => s.reset);
  const newSession = useManagerStore((s) => s.newSession);
  const tasks = useTasksStore((s) => s.tasks);
  const loadTasks = useTasksStore((s) => s.loadTasks);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [configOpen, setConfigOpen] = useState(false);
  const [mgrDraft, setMgrDraft] = useState<Agent | null>(null);
  const avatarRef = useRef<HTMLInputElement>(null);

  // Custom avatar: read the file as a data URL and keep it on the agent record.
  const onPickAvatar = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    if (file.size > 2 * 1024 * 1024) { toast('image must be under 2 MB', 'error'); return; }
    const reader = new FileReader();
    reader.onload = () => setMgrDraft((d) => (d ? { ...d, avatar: String(reader.result ?? '') } : d));
    reader.onerror = () => toast('could not read that image', 'error');
    reader.readAsDataURL(file);
  };
  const [sessions, setSessions] = useState<{ id: string; title: string; createdAt: string; updatedAt: string }[]>([]);
  const [viewingSession, setViewingSession] = useState<string | null>(null);
  const [viewMsgs, setViewMsgs] = useState<ChatEntry[]>([]);
  const sessionId = useManagerStore((s) => s.sessionIds[managerId] ?? null);
  const persistAgent = useAgentsStore((s) => s.persistAgent);

  const managerAgent = agents.find((a) => a.isManager) ?? {
    id: 'manager', name: 'Manager', objective: '', model: models.find((m) => m.enabled)?.id ?? '', toolIds: [],
    integrations: [], skillIds: [], memory: true, permissions: ['network'], homePath: 'agents/manager', color: '', x: 0, y: 0, isManager: true,
  };

  const openConfig = () => {
    const m = agents.find((a) => a.isManager) ?? managerAgent;
    setMgrDraft({ ...m });
    setConfigOpen(true);
  };

  const saveConfig = async () => {
    if (!mgrDraft) return;
    await persistAgent(mgrDraft);
    toast(`${leadName} config saved`, 'success');
    setConfigOpen(false);
  };

  useEffect(() => { loadTasks(); }, [loadTasks]);
  useEffect(() => {
    loadHistory(managerId);
    listChatSessions(managerId).then(setSessions).catch(() => {});
  }, [managerId, loadHistory]);
  useStickToBottom(scrollRef, messages);
  // "Settings" from the sidebar's context menu reopens the config panel.
  useEffect(() => {
    if (openConfigRequest > 0) openConfig();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openConfigRequest]);

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
      return `Available commands:\n- /agents — list agents\n- /tasks — list tasks\n- /switch &lt;agent&gt; — switch conversation to an agent\n- /help — this message\n\nEverything else goes to ${leadName}.`;
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

  const submit = async (text: string) => {
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
  const [tab, setTab] = useState<'chat' | 'info'>('chat');

  const menuItems: MenuItem[] = [
    { key: 'chat', label: 'chat', icon: <MessageSquare size={13} />, active: tab === 'chat', onSelect: () => setTab('chat') },
    { key: 'info', label: 'info', icon: <Info size={13} />, active: tab === 'info', onSelect: () => setTab('info') },
    { key: 'new', label: 'new chat', icon: <MessageSquarePlus size={13} />, dividerBefore: true, onSelect: () => {
      const m = agents.find((a) => a.id === managerId)?.model ?? models.find((x) => x.enabled)?.id ?? '';
      void newSession(managerId, m);
    } },
    { key: 'config', label: 'config', icon: <Settings size={13} />, onSelect: openConfig },
    { key: 'reset', label: 'reset memory', icon: <RotateCcw size={13} />, onSelect: () => { void reset(managerId); } },
  ];

  return (
    <div className="flex h-full flex-col gap-1">
      <header className="flex shrink-0 items-center justify-end">
        <ContextMenu items={menuItems} />
      </header>

      {tab === 'chat' && (
        <div className="relative flex min-h-0 flex-1 flex-col">
          <div ref={scrollRef} className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-4 pt-4 pb-32">
            <div className="mx-auto w-full max-w-[720px]">
              {messages.map((m, i) => (
                <MessageBubble
                  key={i}
                  role={m.role}
                  content={m.content}
                  steps={steps}
                  streaming={busy && i === messages.length - 1 && m.role === 'assistant'}
                />
              ))}
            </div>
          </div>
          <ChatComposer
            busy={busy}
            modelId={managerAgent.model}
            placeholder={`message ${leadName}…  (/agents, /tasks, /switch, /help)`}
            onSend={submit}
            onModelChange={(id) => void persistAgent({ ...managerAgent, model: id })}
          />
        </div>
      )}

      {tab === 'info' && (
        <div className="grid min-h-0 flex-1 grid-cols-1 content-start gap-3 overflow-x-hidden overflow-y-auto md:grid-cols-2 xl:grid-cols-3">
          {/* Agents */}
          <div className="rounded-xl border border-border bg-surface p-3.5">
            <span className={GROUP_LABEL_CLS}>agents</span>
            <div className="mt-2 grid gap-0.5">
              {agents.filter((a) => !a.isManager).map((a) => (
                <button
                  key={a.id}
                  className={`focus-ring cursor-pointer rounded-lg border-0 px-2.5 py-1.5 text-left font-mono text-[11px] transition-colors ${currentAgentId === a.id ? 'bg-background text-foreground' : 'text-muted hover:bg-background hover:text-foreground'}`}
                  onClick={() => setCurrentAgent(a.id)}
                >
                  {a.name}
                  <span className="block truncate font-mono text-[10px] text-muted">{a.integrations.join(', ') || 'no integrations'}</span>
                </button>
              ))}
              {agents.filter((a) => !a.isManager).length === 0 && <p className="m-0 px-2 font-mono text-[10px] text-muted">no agents yet</p>}
            </div>
          </div>

          {/* Integrations */}
          <div className="rounded-xl border border-border bg-surface p-3.5">
            <span className={GROUP_LABEL_CLS}>integrations</span>
            <div className="mt-2 grid gap-0.5">
              {integrations.map((i) => (
                <div key={i.id} className="flex items-center gap-2 rounded-lg px-2.5 py-1.5 font-mono text-[11px]">
                  <span className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${i.connected ? 'bg-foreground' : 'bg-muted'}`} />
                  <span className="min-w-0 truncate text-muted">{i.name}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Active tasks */}
          <div className="rounded-xl border border-border bg-surface p-3.5">
            <span className={GROUP_LABEL_CLS}>active tasks</span>
            <div className="mt-2 grid gap-1.5">
              {activeTasks.length === 0 && <p className="m-0 px-2 font-mono text-[10px] text-muted">no active tasks</p>}
              {activeTasks.map((t: Task) => (
                <div key={t.id} className="rounded-lg border border-border bg-background p-2">
                  <div className="flex items-center justify-between gap-2">
                    <b className="min-w-0 truncate font-mono text-[11px] text-foreground">{agents.find((a) => a.id === t.assignedAgent)?.name ?? t.assignedAgent}</b>
                    <StatusTag status={t.status} />
                  </div>
                  <p className="mt-1 mb-0 line-clamp-2 font-mono text-[10px] leading-relaxed text-muted">{t.input}</p>
                </div>
              ))}
            </div>
          </div>

          {/* Chats */}
          <div className="rounded-xl border border-border bg-surface p-3.5">
            <span className={GROUP_LABEL_CLS}>chats</span>
            <div className="mt-2 grid gap-1.5">
              {sessions.length === 0 && <p className="m-0 px-2 font-mono text-[10px] text-muted">no chats yet</p>}
              {sessions.map((s) => (
                <div key={s.id} className={`flex items-center gap-1 rounded-lg border px-2 py-1.5 ${s.id === sessionId ? 'border-foreground/40 bg-background' : 'border-border bg-background'}`}>
                  <button className="min-w-0 flex-1 cursor-pointer overflow-hidden border-0 bg-transparent p-0 text-left" onClick={async () => {
                    setViewingSession(s.id);
                    const msgs = await getChatSession(s.id);
                    setViewMsgs(msgs.map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content, time: '' })));
                  }}>
                    <span className={`block truncate font-mono text-[11px] ${s.id === sessionId ? 'text-foreground' : 'text-muted'}`}>{s.id === sessionId ? 'current chat' : s.title}</span>
                    <span className="block font-mono text-[10px] text-muted">{s.updatedAt ? (() => { const d = new Date(s.updatedAt); return isNaN(d.getTime()) ? '' : d.toLocaleString(); })() : ''}</span>
                  </button>
                  <button className="cursor-pointer border-0 bg-transparent p-1 text-muted transition-colors hover:text-danger" onClick={async () => { await deleteChatSession(s.id); listChatSessions(agents.find((a) => a.isManager)?.id ?? 'manager').then(setSessions); }}><Trash2 size={11} /></button>
                </div>
              ))}
            </div>
          </div>

          {/* Commands */}
          <div className="rounded-xl border border-border bg-surface p-3.5">
            <span className={GROUP_LABEL_CLS}>commands</span>
            <div className="mt-2 grid gap-1">
              <span className="rounded bg-background px-2 py-1.5 font-mono text-[11px] text-muted">/switch &lt;agent&gt;</span>
              <span className="rounded bg-background px-2 py-1.5 font-mono text-[11px] text-muted">/agents</span>
              <span className="rounded bg-background px-2 py-1.5 font-mono text-[11px] text-muted">/tasks</span>
              <span className="rounded bg-background px-2 py-1.5 font-mono text-[11px] text-muted">/help</span>
              <CornerDownLeft size={12} className="mt-1 text-muted opacity-50" />
            </div>
          </div>
        </div>
      )}

      {configOpen && mgrDraft && (
        <Modal
          title={`${leadName} config`}
          onClose={() => setConfigOpen(false)}
          headerAction={<Button variant="primary" onClick={saveConfig}>save</Button>}
        >
          <label className={FIELD_LABEL_CLS}>name</label>
          <input
            value={mgrDraft.name}
            onChange={(e) => setMgrDraft({ ...mgrDraft, name: e.target.value })}
            placeholder="Name your lead agent"
            className={INPUT_CLS}
          />

          <label className={fieldLabel}>model</label>
          <Select
            value={mgrDraft.model}
            options={models.map((m) => ({ value: m.id, label: m.label }))}
            onChange={(v) => setMgrDraft({ ...mgrDraft, model: v })}
          />

          <label className={fieldLabel}>persona</label>
          <PersonaPicker value={mgrDraft.persona ?? 'ai-orb'} onChange={(v) => setMgrDraft({ ...mgrDraft, persona: v })} />
          <div className="mt-2 flex items-center gap-2">
            <AgentAvatar agent={{ ...mgrDraft, persona: mgrDraft.persona ?? 'ai-orb' }} size={28} />
          </div>

          <label className={fieldLabel}>avatar</label>
          <div className="mt-1.5 flex items-center gap-2">
            <input ref={avatarRef} type="file" accept="image/png,image/jpeg,image/gif,image/webp" className="hidden" onChange={onPickAvatar} />
            <Button onClick={() => avatarRef.current?.click()}>upload image</Button>
            {mgrDraft.avatar && (
              <Button onClick={() => setMgrDraft({ ...mgrDraft, avatar: '' })}>remove</Button>
            )}
          </div>
          <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">PNG, JPEG, GIF or WebP up to 2 MB. Overrides the persona.</p>

          <label className={fieldLabel}>objective / prompt</label>
          <textarea
            value={mgrDraft.objective}
            onChange={(e) => setMgrDraft({ ...mgrDraft, objective: e.target.value })}
            rows={6}
            placeholder={`Describe ${leadName}'s role…`}
            className={PROSE_CLS}
          />

          <label className={fieldLabel}>permissions</label>
          <div className="-mx-2 mt-1.5">
            {(['network', 'files', 'host_fs'] as const).map((p) => (
              <Checkbox
                key={p}
                checked={mgrDraft.permissions.includes(p)}
                onChange={(next) => setMgrDraft({ ...mgrDraft, permissions: next ? [...mgrDraft.permissions, p] : mgrDraft.permissions.filter((x) => x !== p) })}
                label={p}
              />
            ))}
          </div>

          <label className={fieldLabel}>memory</label>
          <div className="mt-1.5 flex items-center gap-2">
            <Button onClick={() => { const id = mgrDraft.id; reset(id); toast(`${leadName} memory cleared`, 'success'); }}>reset memory</Button>
          </div>
        </Modal>
      )}

      {viewingSession && (
        <Modal title="chat history" onClose={() => setViewingSession(null)} width="min(92vw, 560px)">
          {viewMsgs.length === 0
            ? <p className="m-0 text-center font-mono text-xs text-muted">no messages</p>
            : viewMsgs.map((m, i) => <MessageBubble key={i} role={m.role} content={m.content} />)}
        </Modal>
      )}
    </div>
  );
}
