import { useEffect, useRef, useState } from 'react';
import { AlertTriangle, ArrowLeft, Check, ChevronRight, Lock, MessageSquarePlus, RotateCcw, Settings, Trash2 } from 'lucide-react';
import { ContextMenu, type MenuItem } from './ui/ContextMenu';
import type { Agent, ChatMessage, ExecutionResult, Integration, ModelConfig, Run, Skill, Tool } from '../types';
import { Select } from './ui/Select';
import { MultiDropdown } from './ui/MultiDropdown';
import { AgentAvatar, PersonaPicker } from './ui/AgentAvatar';
import { Button } from './ui/Button';
import { StatusGlyph, StatusTag, statusClass } from './ui/Status';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS, PROSE_CLS } from './ui/Input';
import { ChatComposer } from './chat/ChatComposer';
import { MessageBubble } from './chat/MessageBubble';
import { toast } from '../hooks/useToast';
import { listChatSessions, getChatSession, createChatSession, deleteChatSession, closeSession, streamChat } from '../runtime';
import { useRunsStore } from '../hooks/useRuns';
import { useManagerStore, type ChatEntry } from '../hooks/useManager';
import { useConfirmStore } from '../hooks/useConfirm';
import { useShallow } from 'zustand/react/shallow';
import { DeleteConfirm } from './ui/DeleteConfirm';
import { Checkbox } from './ui/Checkbox';
import { createDeltaBuffer } from '../streamBuffer';

const isComplete = (a: Agent) => !!a.name.trim() && a.name.trim() !== 'New Agent' && !!a.model.trim() && !!a.objective.trim();

const PERMISSIONS = [
  { key: 'network', label: 'network', hint: 'http / api' },
  { key: 'files', label: 'files', hint: 'sandboxed' },
  { key: 'host_fs', label: 'host filesystem', hint: 'whole device' },
] as const;

// Collapsible config section — collapsed by default, so the rail reads as
// name → model → prompt and everything else is one summarised line until asked
// for. Nothing is hidden, just quiet.
function Section({ title, summary, defaultOpen = false, children }: {
  title: string; summary?: string; defaultOpen?: boolean; children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="border-t border-border pt-4">
      <button type="button" onClick={() => setOpen((o) => !o)} className="focus-ring flex w-full cursor-pointer items-center gap-2 border-0 bg-transparent text-left">
        <span className="shrink-0 font-mono text-[10px] tracking-wider text-muted uppercase">{title}</span>
        <span className="min-w-0 flex-1 truncate text-right font-mono text-[10px] text-muted">{summary}</span>
        <ChevronRight size={13} className={`shrink-0 text-muted transition-transform duration-150 ${open ? 'rotate-90' : ''}`} />
      </button>
      {open && <div className="mt-3.5">{children}</div>}
    </div>
  );
}

// Safe date formatting — DB timestamps can be empty/malformed, and new Date('')
// throws, which would blank the whole screen.
const fmtDate = (s?: string | null) => {
  if (!s) return '';
  const d = new Date(s);
  return isNaN(d.getTime()) ? '' : d.toLocaleString();
};

const runDuration = (r: Run) => {
  if (!r.startedAt) return '';
  const start = new Date(r.startedAt).getTime();
  if (isNaN(start)) return '';
  const end = r.endedAt ? new Date(r.endedAt).getTime() : Date.now();
  if (isNaN(end)) return '';
  const ms = Math.max(0, end - start);
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
};

export function AgentWindow({ agent, tools, skills, models, integrations, runs, onSave, onDelete, onRun, tabRequest }: {
  agent: Agent; tools: Tool[]; skills: Skill[]; models: ModelConfig[]; integrations: Integration[]; runs: Run[];
  onSave: (a: Agent) => Promise<void>; onDelete: (id: string) => Promise<void>; onRun: (input: string, agent: Agent) => Promise<ExecutionResult>;
  tabRequest?: { tab: 'chat' | 'config'; n: number };
}) {
  const complete = isComplete(agent);
  const [tab, setTab] = useState<'chat' | 'runs' | 'info' | 'config' | 'history'>(complete ? 'chat' : 'config');

  // Honour an explicit tab request from the sidebar (Settings / reopen to chat).
  useEffect(() => {
    if (tabRequest && tabRequest.n > 0) setTab(tabRequest.tab);
  }, [tabRequest?.n]);
  // Chat messages live in the store so they survive navigating away and back.
  // useShallow prevents an infinite re-render loop when the conversation is
  // missing (the `?? []` would create a new reference every selector call).
  const messages = useManagerStore(useShallow((s) => s.conversations[agent.id] ?? []));
  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [sessions, setSessions] = useState<{ id: string; title: string; createdAt: string; updatedAt: string }[]>([]);
  const [viewingSession, setViewingSession] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  // Live step shown in the pending assistant bubble instead of a bare "typing".
  const [status, setStatus] = useState('');
  const [error, setError] = useState<string | undefined>(undefined);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [expandedRuns, setExpandedRuns] = useState<Set<string>>(new Set());
  const [draft, setDraft] = useState<Agent>(agent);
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const enabledTools = tools.filter((t) => t.enabled);
  // MCP tools are grouped under their server so the picker reads by source
  // instead of a flat wall of raw tool names. Ungrouped tools come first.
  const toolOptions = [
    ...enabledTools.filter((t) => t.kind !== 'mcp').map((t) => ({ value: t.id, label: t.name })),
    ...enabledTools.filter((t) => t.kind === 'mcp')
      .map((t) => ({ value: t.id, label: t.name, group: `mcp · ${String(t.config.serverName ?? t.integrationId)}` }))
      .sort((a, b) => a.group.localeCompare(b.group) || a.label.localeCompare(b.label)),
  ];
  const scrollRef = useRef<HTMLDivElement>(null);
  const avatarRef = useRef<HTMLInputElement>(null);

  // Custom avatar: read the file as a data URL and keep it on the agent record.
  const onPickAvatar = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    if (file.size > 2 * 1024 * 1024) { toast('image must be under 2 MB', 'error'); return; }
    const reader = new FileReader();
    reader.onload = () => setDraft((d) => ({ ...d, avatar: String(reader.result ?? '') }));
    reader.onerror = () => toast('could not read that image', 'error');
    reader.readAsDataURL(file);
  };

  const setMessages = (fn: (prev: ChatEntry[]) => ChatEntry[]) => {
    useManagerStore.setState((s) => {
      const updated = fn(s.conversations[agent.id] ?? []);
      return { conversations: { ...s.conversations, [agent.id]: updated } };
    });
  };

  // Load any persisted conversation for this agent on mount.
  useEffect(() => {
    useManagerStore.getState().loadHistory(agent.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agent.id]);

  const switchTab = (t: 'chat' | 'runs' | 'info' | 'config' | 'history') => {
    if ((t === 'chat' || t === 'runs') && !complete) { setTab('config'); return; }
    setTab(t);
  };

  const toggleRun = (id: string) => {
    setExpandedRuns((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  useEffect(() => {
    if (tab === 'history') {
      listChatSessions(agent.id).then(setSessions).catch(() => setSessions([]));
      setViewingSession(null);
    }
  }, [tab, agent.id]);

  useEffect(() => {
    // Always scroll the newest message into view.
    const el = scrollRef.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
  }, [messages]);

  const send = async (text: string) => {
    if (!text.trim() || running) return;
    setRunning(true); setError(undefined); setStatus('thinking…');
    const userMsg: ChatEntry = { role: 'user', content: text, time: new Date().toLocaleTimeString() };
    const assistantMsg: ChatEntry = { role: 'assistant', content: '', time: new Date().toLocaleTimeString() };
    setMessages((prev) => [...prev, userMsg, assistantMsg]);
    const agentToRun = agent;
    // Ensure a chat session exists so this agent's messages are recorded and
    // the visible chat stays session-scoped.
    let sid = useManagerStore.getState().sessionIds[agent.id];
    if (!sid) {
      const sess = await createChatSession(agent.id, 'Chat');
      sid = sess.id;
      useManagerStore.setState((s) => ({ sessionIds: { ...s.sessionIds, [agent.id]: sid } }));
    }
    // Batch the streamed deltas — see src/streamBuffer.ts for why.
    const batcher = createDeltaBuffer((text) => {
      setMessages((prev) => prev.map((m, i) => (i === prev.length - 1 ? { ...m, content: text } : m)));
    });
    let failed = false;
    try {
      await streamChat(agentToRun, text, false, (delta) => batcher.push(delta), (confirmReq) => {
        // Agent tools that need approval (e.g. run_command) pop the same panel.
        useConfirmStore.getState().request(confirmReq);
      }, sid, setStatus);
      useRunsStore.getState().loadRuns();
    } catch (e) {
      failed = true;
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      setMessages((prev) => prev.map((m, i) => (i === prev.length - 1 ? { ...m, content: `**run failed:** ${message}` } : m)));
    } finally {
      // Only flush the tail on success, so a late flush can't overwrite the error.
      if (!failed) batcher.end();
      setRunning(false);
      setStatus('');
    }
  };

  const save = async (startChat = false) => {
    if (!draft.name.trim()) { setError('Agent needs a name.'); return; }
    await onSave(draft);
    setError(undefined);
    toast('agent saved', 'success');
    if (startChat && isComplete(draft)) setTab('chat');
  };

  const newChat = async () => {
    // Close the outgoing session so it gets summarized into day context.
    const cur = useManagerStore.getState().sessionIds[agent.id];
    if (cur) { void closeSession(cur, agent.id, agent.model); }
    const sess = await createChatSession(agent.id, 'Chat');
    useManagerStore.setState((s) => ({ sessionIds: { ...s.sessionIds, [agent.id]: sess.id } }));
    setMessages(() => []);
    setViewingSession(null);
    setTab('chat');
  };

  const resetMemory = async () => {
    await useManagerStore.getState().reset(agent.id);
    toast('memory cleared', 'success');
  };

  const menuItems: MenuItem[] = [
    { key: 'config', label: 'settings', icon: <Settings size={13} />, onSelect: () => setTab('config') },
    { key: 'new', label: 'new chat', icon: <MessageSquarePlus size={13} />, onSelect: () => { void newChat(); } },
    { key: 'reset', label: 'reset memory', icon: <RotateCcw size={13} />, onSelect: () => { void resetMemory(); } },
    { key: 'delete', label: 'delete agent', icon: <Trash2 size={13} />, danger: true, onSelect: () => setConfirmDelete(true) },
  ];

  const tabBtn = (active: boolean) =>
    `focus-ring flex cursor-pointer items-center gap-1 rounded-lg border px-2.5 py-1 font-mono text-[11px] lowercase transition-colors ${active ? 'border-border bg-surface text-foreground' : 'border-transparent bg-transparent text-muted hover:bg-surface hover:text-foreground'}`;

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <header className="flex shrink-0 flex-wrap items-center justify-end gap-2">
        <div className="flex items-center gap-1">
          {(['chat', 'runs', 'info', 'config', 'history'] as const).map((t) => {
            const locked = !complete && (t === 'chat' || t === 'runs');
            return (
              <button key={t} className={tabBtn(tab === t)} onClick={() => switchTab(t)}>
                {t}{locked && <Lock size={9} className="opacity-70" />}
              </button>
            );
          })}
          <ContextMenu items={menuItems} />
        </div>
      </header>

      {(tab === 'chat' || tab === 'runs') && !complete && (
        <div className="mt-4 flex min-h-0 flex-1 flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border bg-surface text-center">
          <Lock size={18} className="text-muted" />
          <h3 className="m-0 font-mono text-xs lowercase text-foreground">finish setting up {agent.name}</h3>
          <Button variant="primary" icon={<Check size={13} />} onClick={() => setTab('config')}>go to config</Button>
        </div>
      )}

      {tab === 'chat' && (
        <div className="relative flex min-h-0 flex-1 flex-col">
          {/* Chat history — full height, no box; the composer overlays on top of it */}
          <div ref={scrollRef} className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-4 pt-4 pb-32">
            <div className="mx-auto w-full max-w-[720px]">
              {messages.map((m, i) => (
                <MessageBubble
                  key={i}
                  role={m.role}
                  content={m.content}
                  status={status}
                  streaming={running && i === messages.length - 1 && m.role === 'assistant'}
                />
              ))}
            </div>
          </div>
          {error && (
            <p className="absolute bottom-[76px] left-4 z-30 m-0 flex items-center gap-1.5 font-mono text-[11px] text-danger">
              <AlertTriangle size={11} className="shrink-0" />{error}
            </p>
          )}
          <ChatComposer busy={running} modelId={agent.model} placeholder={`message ${agent.name}…`} onSend={send} />
        </div>
      )}

      {tab === 'runs' && (
        <div className="mt-4 min-h-0 flex-1 overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-background p-3 font-mono text-[11px] leading-relaxed">
          {runs.filter((r) => r.agentId === agent.id).sort((a, b) => new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()).map((r) => {
            const isOpen = expandedRuns.has(r.id);
            const totalTokens = (r.promptTokens ?? 0) + (r.completionTokens ?? 0);
            return (
              <div key={r.id} className="border-b border-border last:border-0">
                <button className="focus-ring flex w-full cursor-pointer items-center gap-3 border-0 bg-transparent px-1 py-2 text-left" onClick={() => toggleRun(r.id)}>
                  <ChevronRight size={11} className={`shrink-0 text-muted transition-transform duration-150 ${isOpen ? 'rotate-90' : ''}`} />
                  <span className={`shrink-0 ${statusClass(r.status)}`}><StatusGlyph status={r.status} /></span>
                  <span className="shrink-0 text-muted">{fmtDate(r.startedAt)}</span>
                  <span className="min-w-0 truncate text-muted">{r.model}</span>
                  <span className="shrink-0 text-muted">{runDuration(r)}</span>
                  {totalTokens > 0 && <span className="shrink-0 text-muted">{totalTokens.toLocaleString()} tok</span>}
                  <span className="ml-auto shrink-0 lowercase"><StatusTag status={r.status} /></span>
                </button>
                {isOpen && (
                  <div className="px-1 pb-3">
                    <div className="my-1 text-foreground/80">$ {r.input}</div>
                    {(r.events ?? []).map((ev, i) => (
                      <div key={i} className="flex items-baseline gap-2">
                        <span className="flex-none text-muted">{ev.time}</span>
                        <span className="w-10 flex-none text-muted">{ev.type === 'tool' ? 'tool' : ev.type === 'thought' ? 'think' : 'out'}</span>
                        <span className="min-w-0 flex-1 text-foreground/70">{ev.title}{ev.detail ? ` — ${ev.detail}` : ''}</span>
                      </div>
                    ))}
                    {r.output && <pre className="mt-1.5 ml-12 max-w-full overflow-x-hidden whitespace-pre-wrap rounded border border-border bg-background p-2 text-[11px] text-foreground">{r.output}</pre>}
                    {r.status === 'failed' && (
                      <div className="flex items-baseline gap-2">
                        <span className="flex-none text-muted" />
                        <span className="w-10 flex-none text-danger">err</span>
                        <span className="min-w-0 flex-1 text-muted">run failed — see agent chat for details.</span>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {tab === 'info' && (
        <div className="mt-4 min-h-0 flex-1 overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-surface p-4">
          <div className="mb-2 flex justify-between"><span className={GROUP_LABEL_CLS}>tools</span></div>
          <div className="flex flex-wrap gap-1.5">{agent.toolIds.map((t) => <span key={t} className="rounded bg-background px-2 py-1 font-mono text-[10px] text-muted">{toolName(t)}</span>)}</div>
          <div className="mt-5 mb-2 flex justify-between"><span className={GROUP_LABEL_CLS}>integrations</span></div>
          <div className="flex flex-wrap gap-1.5">{agent.integrations.map((t) => <span key={t} className="rounded bg-background px-2 py-1 font-mono text-[10px] text-muted">{t}</span>)}</div>
          <div className="mt-5 mb-2 flex justify-between"><span className={GROUP_LABEL_CLS}>permissions</span></div>
          <div className="rounded-lg bg-background p-2.5 font-mono text-[10px] text-muted">{agent.permissions.join(', ') || 'none'}</div>
          <div className="mt-5 mb-2 flex justify-between"><span className={GROUP_LABEL_CLS}>home</span></div>
          <div className="rounded-lg bg-background p-2.5 font-mono text-[10px] text-muted">{agent.homePath}</div>
          <div className="mt-5 mb-2 flex justify-between"><span className={GROUP_LABEL_CLS}>model</span></div>
          <div className="rounded-lg bg-background p-2.5 font-mono text-[10px] text-muted">{agent.model}</div>
        </div>
      )}

      {tab === 'history' && (
        <div className="mt-4 min-h-0 flex-1 overflow-x-hidden overflow-y-auto rounded-xl border border-border bg-surface p-4">
          {viewingSession ? (
            <div>
              <div className="mb-3 flex items-center justify-between gap-2">
                <Button icon={<ArrowLeft size={12} />} onClick={() => setViewingSession(null)}>all chats</Button>
                <Button onClick={async () => { await deleteChatSession(viewingSession); setViewingSession(null); listChatSessions(agent.id).then(setSessions); }}>delete chat</Button>
              </div>
              <div className="grid gap-3">
                {history.map((m, i) => (
                  <MessageBubble key={i} role={m.role === 'user' ? 'user' : 'assistant'} content={m.content} meta={m.role === 'user' ? 'you' : agent.name} />
                ))}
              </div>
            </div>
          ) : (
            <div>
              <span className={GROUP_LABEL_CLS}>past chats</span>
              {sessions.length === 0 ? null : (
                <div className="mt-3 grid gap-1.5">
                  {sessions.map((s) => (
                    <button key={s.id} className="focus-ring flex cursor-pointer items-center justify-between gap-3 rounded-lg border border-border bg-background px-3 py-2 text-left transition-colors hover:border-foreground/30" onClick={async () => { setViewingSession(s.id); const msgs = await getChatSession(s.id); setHistory(msgs.map((m, i) => ({ role: m.role as 'user' | 'assistant', content: m.content, time: `#${i + 1}` }))); }}>
                      <span className="min-w-0 truncate font-mono text-[11px] text-foreground">{s.title}</span>
                      <span className="shrink-0 font-mono text-[10px] text-muted">{fmtDate(s.updatedAt)}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {tab === 'config' && (
        <div className="mt-5 grid min-h-0 flex-1 grid-cols-1 items-stretch gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
          {/* Prompt: content, so it leads and takes the dominant column */}
          <textarea
            value={draft.objective}
            onChange={(e) => setDraft({ ...draft, objective: e.target.value })}
            placeholder="Prompt — describe what this agent should do, its role, how it should behave, what format to return…"
            className={`${PROSE_CLS} h-full`}
          />

          {/* Sister rail: everything else, save pinned to its bottom */}
          <div className="relative flex min-h-0 flex-col overflow-hidden rounded-xl border border-border bg-surface">
            <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-x-hidden overflow-y-auto p-4 pb-16">
              <div>
                <label className={FIELD_LABEL_CLS}>name</label>
                <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} placeholder="Agent name" className={INPUT_CLS} />
              </div>

              <div>
                <label className={FIELD_LABEL_CLS}>model</label>
                <Select
                  value={draft.model}
                  options={models.map((m) => ({ value: m.id, label: m.label }))}
                  onChange={(v) => setDraft({ ...draft, model: v })}
                />
              </div>

              <Section title="appearance" summary={draft.avatar ? 'custom image' : 'persona'}>
                <div className="flex items-center gap-3">
                  <AgentAvatar agent={{ ...draft, persona: draft.persona ?? 'ai-orb' }} size={40} playing />
                  <div className="min-w-0 flex-1">
                    <PersonaPicker value={draft.persona ?? 'ai-orb'} onChange={(v) => setDraft({ ...draft, persona: v })} />
                  </div>
                </div>
                <div className="mt-3 flex items-center gap-2">
                  <input ref={avatarRef} type="file" accept="image/png,image/jpeg,image/gif,image/webp" className="hidden" onChange={onPickAvatar} />
                  <Button onClick={() => avatarRef.current?.click()}>upload image</Button>
                  {draft.avatar && <Button onClick={() => setDraft({ ...draft, avatar: '' })}>remove</Button>}
                </div>
                <p className="mt-2 mb-0 font-mono text-[10px] leading-relaxed text-muted">PNG, JPEG, GIF or WebP up to 2 MB — replaces the persona.</p>
              </Section>

              <Section
                title="capabilities"
                summary={`${draft.toolIds.length} tools · ${(draft.skillIds ?? []).length} skills · ${draft.integrations.length} integrations`}
              >
                <label className={FIELD_LABEL_CLS}>tools</label>
                <MultiDropdown
                  values={draft.toolIds}
                  options={toolOptions}
                  onChange={(v) => setDraft({ ...draft, toolIds: v })}
                  placeholder="none attached"
                />

                <label className={`${FIELD_LABEL_CLS} mt-4`}>skills</label>
                <MultiDropdown
                  values={draft.skillIds ?? []}
                  options={skills.map((s) => ({ value: s.id, label: s.name }))}
                  onChange={(v) => setDraft({ ...draft, skillIds: v })}
                  placeholder="none attached"
                />

                <label className={`${FIELD_LABEL_CLS} mt-4`}>integrations</label>
                {integrations.filter((i) => i.connected).length === 0 ? (
                  <p className="my-0 px-2 py-1 font-mono text-[10px] leading-relaxed text-muted">none connected yet</p>
                ) : (
                  <div>
                    {integrations.filter((i) => i.connected).map((i) => (
                      <Checkbox
                        key={i.id}
                        checked={draft.integrations.includes(i.id)}
                        onChange={(next) => setDraft({ ...draft, integrations: next ? [...draft.integrations, i.id] : draft.integrations.filter((x) => x !== i.id) })}
                        label={i.name}
                        hint={`${i.actions.length} actions`}
                      />
                    ))}
                  </div>
                )}
              </Section>

              <Section title="permissions" summary={draft.permissions.join(', ') || 'none'}>
                <div>
                  {PERMISSIONS.map((p) => (
                    <Checkbox
                      key={p.key}
                      checked={draft.permissions.includes(p.key)}
                      onChange={(next) => setDraft({ ...draft, permissions: next ? [...draft.permissions, p.key] : draft.permissions.filter((x) => x !== p.key) })}
                      label={p.label}
                      hint={p.hint}
                    />
                  ))}
                </div>
              </Section>

              {error && <p className="m-0 font-mono text-[10px] leading-relaxed text-danger">{error}</p>}
            </div>

            {/* The save bar sits at the rail's bottom; content scrolls beneath it */}
            <div className="absolute right-0 bottom-0 left-0 flex items-center gap-3 border-t border-border bg-surface px-4 py-3">
              <Button
                variant="primary"
                icon={<Check size={13} />}
                onClick={() => save(true)}
                disabled={!isComplete(draft) || JSON.stringify(draft) === JSON.stringify(agent)}
              >
                {isComplete(agent) ? 'save changes' : 'save & start chatting'}
              </Button>
              {JSON.stringify(draft) !== JSON.stringify(agent) && isComplete(draft) && (
                <span className="font-mono text-[10px] text-muted">unsaved</span>
              )}
            </div>
          </div>
        </div>
      )}

      {confirmDelete && (
        <DeleteConfirm
          name={agent.name}
          description="This permanently removes the agent and its configuration. Type the agent's name to confirm."
          onCancel={() => setConfirmDelete(false)}
          onConfirm={() => { setConfirmDelete(false); void onDelete(agent.id); }}
        />
      )}
    </div>
  );
}
