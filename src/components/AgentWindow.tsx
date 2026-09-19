import { useEffect, useRef, useState } from 'react';
import { Check, ChevronRight, Lock, MessageSquarePlus, Play, RotateCcw, Send, Settings, Trash2 } from 'lucide-react';
import { ContextMenu, type MenuItem } from './ui/ContextMenu';
import type { Agent, ChatMessage, ExecutionResult, Integration, ModelConfig, Run, Skill, Tool } from '../types';
import { Dropdown } from './ui/Dropdown';
import { MultiDropdown } from './ui/MultiDropdown';
import { AgentAvatar, PersonaPicker } from './ui/AgentAvatar';
import { Markdown } from './ui/Markdown';
import { toast } from '../hooks/useToast';
import { listChatSessions, getChatSession, createChatSession, deleteChatSession, closeSession, streamChat } from '../runtime';
import { useRunsStore } from '../hooks/useRuns';
import { useManagerStore, type ChatEntry } from '../hooks/useManager';
import { useConfirmStore } from '../hooks/useConfirm';
import { useShallow } from 'zustand/react/shallow';
import { StreamIndicator } from './ui/StreamIndicator';
import { ConfirmDialog } from './ui/ConfirmDialog';

const isComplete = (a: Agent) => !!a.name.trim() && a.name.trim() !== 'New Agent' && !!a.model.trim() && !!a.objective.trim();

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

export function AgentWindow({ agent, tools, skills, models, integrations, runs, onBack, onSave, onDelete, onRun, tabRequest }: {
  agent: Agent; tools: Tool[]; skills: Skill[]; models: ModelConfig[]; integrations: Integration[]; runs: Run[];
  onBack: () => void; onSave: (a: Agent) => Promise<void>; onDelete: (id: string) => Promise<void>; onRun: (input: string, agent: Agent) => Promise<ExecutionResult>;
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
  const [input, setInput] = useState('');
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const [expandedRuns, setExpandedRuns] = useState<Set<string>>(new Set());
  const [draft, setDraft] = useState<Agent>(agent);
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const enabledTools = tools.filter((t) => t.enabled);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const avatarRef = useRef<HTMLInputElement>(null);

  // Custom avatar: read the file as a data URL and keep it on the agent record.
  const onPickAvatar = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    if (file.size > 2 * 1024 * 1024) { toast('Image must be under 2 MB', 'error'); return; }
    const reader = new FileReader();
    reader.onload = () => setDraft((d) => ({ ...d, avatar: String(reader.result ?? '') }));
    reader.onerror = () => toast('Could not read that image', 'error');
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

  // Auto-grow the input up to a max height as the user types.
  const onInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    e.target.style.height = 'auto';
    e.target.style.height = `${Math.min(e.target.scrollHeight, 160)}px`;
  };

  const send = async (text: string) => {
    if (!text.trim() || running) return;
    setRunning(true); setError(undefined);
    const userMsg: ChatEntry = { role: 'user', content: text, time: new Date().toLocaleTimeString() };
    const assistantMsg: ChatEntry = { role: 'assistant', content: '', time: new Date().toLocaleTimeString() };
    setMessages((prev) => [...prev, userMsg, assistantMsg]);
    setInput('');
    // Reset the auto-grown height after clearing.
    if (inputRef.current) inputRef.current.style.height = 'auto';
    const agentToRun = agent;
    // Ensure a chat session exists so this agent's messages are recorded and
    // the visible chat stays session-scoped.
    let sid = useManagerStore.getState().sessionIds[agent.id];
    if (!sid) {
      const sess = await createChatSession(agent.id, 'Chat');
      sid = sess.id;
      useManagerStore.setState((s) => ({ sessionIds: { ...s.sessionIds, [agent.id]: sid } }));
    }
    let buffer = '';
    try {
      await streamChat(agentToRun, text, false, (delta) => {
        buffer += delta;
        setMessages((prev) => prev.map((m, i) => (i === prev.length - 1 ? { ...m, content: buffer } : m)));
      }, (confirmReq) => {
        // Agent tools that need approval (e.g. run_command) pop the same panel.
        useConfirmStore.getState().request(confirmReq);
      }, sid);
      useRunsStore.getState().loadRuns();
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      setMessages((prev) => prev.map((m, i) => (i === prev.length - 1 ? { ...m, content: `⚠️ ${message}` } : m)));
    } finally {
      setRunning(false);
    }
  };

  const save = async (startChat = false) => {
    if (!draft.name.trim()) { setError('Agent needs a name.'); return; }
    await onSave(draft);
    setError(undefined);
    toast('Agent saved', 'success');
    if (startChat && isComplete(draft)) setTab('chat');
  };

  const statusColor = (s: string) => s === 'running' ? 'text-[#facc15]' : s === 'completed' ? 'text-[#22c55e]' : 'text-[#f87171]';

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
    toast('Memory cleared', 'success');
  };

  const menuItems: MenuItem[] = [
    { key: 'config', label: 'Settings', icon: <Settings size={13} />, onSelect: () => setTab('config') },
    { key: 'new', label: 'New chat', icon: <MessageSquarePlus size={13} />, onSelect: () => { void newChat(); } },
    { key: 'reset', label: 'Reset memory', icon: <RotateCcw size={13} />, onSelect: () => { void resetMemory(); } },
    { key: 'delete', label: 'Delete agent', icon: <Trash2 size={13} />, danger: true, onSelect: () => { void onDelete(agent.id); } },
  ];

  return (
    <div className="boxy flex h-full min-h-0 flex-col overflow-hidden">
      <header className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          <button className="cursor-pointer border-0 bg-none p-0 font-mono text-[11px] uppercase tracking-[1px] text-muted hover:text-text" onClick={onBack}>Agents</button>
          <ChevronRight size={12} className="text-mid" />
          <span className="font-mono text-[11px] uppercase tracking-[1px] text-text">{agent.name}</span>
        </div>
        <div className="flex items-center gap-1">
          {(['chat', 'runs', 'info', 'config', 'history'] as const).map((t) => {
            const locked = !complete && (t === 'chat' || t === 'runs');
            return (
              <button key={t} className={`flex cursor-pointer items-center gap-1 rounded-[10px] border px-2.5 py-1.5 text-[11px] capitalize ${tab === t ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`} onClick={() => switchTab(t)}>
                {t}{locked && <Lock size={9} className="opacity-70" />}
              </button>
            );
          })}
          <ContextMenu items={menuItems} />
        </div>
      </header>

      {(tab === 'chat' || tab === 'runs') && !complete && (
        <div className="mt-4 flex h-[calc(100vh-190px)] min-h-[420px] flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-line bg-panel text-center">
          <Lock size={20} className="text-muted" />
          <h3 style={{ margin: 0, fontSize: 14 }}>Finish setting up {agent.name}</h3>
          <button className="primary" onClick={() => setTab('config')}><Check size={13} />Go to config</button>
        </div>
      )}

      {tab === 'chat' && (
        <div className="relative flex h-[calc(100vh-120px)] min-h-[460px] flex-col">
          {/* Chat history — full height, no box; the input overlays on top of it */}
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
            <div className="chat-log flex-1 scrollbar-thin scrollbar-color-mid overflow-y-auto px-4 pt-4 pb-24" ref={scrollRef}>
              {messages.map((m, i) => (
                <div key={i} className={`mb-3 flex ${m.role === 'user' ? 'justify-end' : 'justify-start'} last:mb-0`}>
                  <div className={`max-w-[78%] rounded-[16px] px-3.5 py-2.5 text-[13.5px] leading-[1.6] break-words ${m.role === 'user' ? 'whitespace-pre-wrap rounded-tr-[8px] bg-line text-text' : 'rounded-tl-[8px] border border-line bg-panel2'}`}>
                    {m.role === 'user' ? m.content : (
                      <Markdown>{m.content}</Markdown>
                    )}
                    {running && i === messages.length - 1 && m.role === 'assistant' && (
                      m.content ? <span className="ml-0.5 inline-block h-3.5 w-[2px] animate-pulse bg-muted align-middle" /> : <StreamIndicator streaming />
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
          {error && <p className="absolute bottom-[68px] left-4 text-[11px] text-[#f87171]" style={{ margin: 0 }}>{error}</p>}
          {/* Input overlays the chat, floating at the bottom */}
          <div className="absolute right-0 bottom-0 left-0 flex items-end gap-3 bg-gradient-to-t from-[var(--color-bg)] via-[var(--color-bg)]/85 to-transparent p-3 pt-6">
            <div className="absolute right-3 bottom-[calc(100%+8px)] left-3 z-[30]">
              <ConfirmDialog />
            </div>
            <textarea
              ref={inputRef}
              rows={1} placeholder={`Message ${agent.name}…`}
              value={input}
              onChange={onInputChange}
              onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(input); } }}
              style={{ flex: 1, background: 'var(--panel2)', border: '1px solid var(--color-hairline)', borderRadius: 16, padding: '12px 16px', color: 'var(--text)', resize: 'none', minHeight: 44, maxHeight: 160, boxShadow: 'var(--shadow-soft)' }}
            />
            <button className="primary" disabled={running || !input.trim()} onClick={() => send(input)} title="Send">
              <Send size={14} />
            </button>
          </div>
        </div>
      )}

      {tab === 'runs' && (
        <div className="runs-console mt-4 h-[calc(100vh-190px)] min-h-[420px] overflow-y-auto rounded-lg border border-line bg-inset p-3.5 font-mono text-[12px] leading-[1.6]">
          {runs.filter((r) => r.agentId === agent.id).sort((a, b) => new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()).map((r) => {
            const isOpen = expandedRuns.has(r.id);
            const totalTokens = (r.promptTokens ?? 0) + (r.completionTokens ?? 0);
            return (
              <div key={r.id} className="border-b border-[#1c1c1f] last:border-0">
                <button className="flex w-full cursor-pointer items-center gap-3 border-0 bg-transparent px-1 py-2.5 text-left" onClick={() => toggleRun(r.id)}>
                  <ChevronRight size={11} className={`shrink-0 text-mid transition-transform duration-150 ${isOpen ? 'rotate-90' : ''}`} />
                  <span className={`shrink-0 text-muted ${statusColor(r.status)}`}>{r.status === 'running' ? '▸' : r.status === 'completed' ? '✓' : '✕'}</span>
                  <span className="shrink-0 text-mid">{fmtDate(r.startedAt)}</span>
                  <span className="shrink-0 text-muted">{r.model}</span>
                  <span className="shrink-0 text-mid">{runDuration(r)}</span>
                  {totalTokens > 0 && <span className="shrink-0 text-mid">{totalTokens.toLocaleString()} tok</span>}
                  <span className={`ml-auto shrink-0 text-[11px] ${statusColor(r.status)}`}>{r.status}</span>
                </button>
                {isOpen && (
                  <div className="px-1 pb-3">
                    <div className="my-1 text-[#d4d4d8]">$ {r.input}</div>
                    {(r.events ?? []).map((ev, i) => (
                      <div key={i} className="console-line flex items-baseline gap-2">
                        <span className="flex-none text-mid">{ev.time}</span>
                        <span className={`w-10 flex-none text-muted ${ev.type === 'tool' ? 'text-[#38bdf8]' : ev.type === 'thought' ? 'text-[#c4b5fd]' : 'text-[#22c55e]'}`}>{ev.type === 'tool' ? 'tool' : ev.type === 'thought' ? 'think' : 'out'}</span>
                        <span className={`text-[#a1a1aa] ${ev.type === 'tool' ? 'text-[#7dd3fc]' : ev.type === 'thought' ? 'text-[#c4b5fd]' : ''}`}>{ev.title}{ev.detail ? ` — ${ev.detail}` : ''}</span>
                      </div>
                    ))}
                    {r.output && <pre className="mt-1.5 ml-12 whitespace-pre-wrap rounded-[10px] border border-[#1c1c1f] bg-[#111113] p-2 text-[11px] text-[#e4e4e7]">{r.output}</pre>}
                    {r.status === 'failed' && <div className="console-line flex items-baseline gap-2"><span className="flex-none text-mid" /><span className="w-10 flex-none text-[#22c55e]">err</span><span className="text-[#a1a1aa]">Run failed — see agent chat for details.</span></div>}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {tab === 'info' && (
        <div className="home-content mt-4 rounded-lg border border-line bg-panel p-[22px]">
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold first:mt-0"><span>TOOLS</span></div>
          <div className="flex flex-wrap gap-[5px]">{agent.toolIds.map((t) => <span key={t} className="rounded bg-panel2 px-[7px] py-[5px] text-[11px]">{toolName(t)}</span>)}</div>
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold"><span>INTEGRATIONS</span></div>
          <div className="flex flex-wrap gap-[5px]">{agent.integrations.map((t) => <span key={t} className="rounded bg-panel2 px-[7px] py-[5px] text-[11px]">{t}</span>)}</div>
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold"><span>PERMISSIONS</span></div>
          <div className="mt-2 flex items-center gap-3 rounded-[10px] bg-panel2 p-2.5 text-[11px]"><span>{agent.permissions.join(', ') || 'none'}</span></div>
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold"><span>HOME</span></div>
          <div className="flex items-center gap-3 rounded-[10px] bg-panel2 p-2.5 text-[11px]"><span>{agent.homePath}</span></div>
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold"><span>MODEL</span></div>
          <div className="flex items-center gap-3 rounded-[10px] bg-panel2 p-2.5 text-[11px]"><span>{agent.model}</span></div>
        </div>
      )}

      {tab === 'history' && (
        <div className="mt-4 max-h-[calc(100vh-280px)] overflow-y-auto rounded-lg border border-line bg-panel p-4">
          {viewingSession ? (
            <div>
              <div className="mb-3 flex items-center justify-between">
                <button className="secondary" onClick={() => setViewingSession(null)}>← All chats</button>
                <button className="secondary" onClick={async () => { await deleteChatSession(viewingSession); setViewingSession(null); listChatSessions(agent.id).then(setSessions); }}>Delete chat</button>
              </div>
              <div className="grid gap-2">
                {history.map((m, i) => (
                  <div key={i} className={`flex ${m.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                    <div className={`max-w-[78%] rounded-[16px] px-3 py-2 text-[12.5px] leading-1.6 break-words ${m.role === 'user' ? 'whitespace-pre-wrap rounded-tr-[8px] bg-line text-text' : 'rounded-tl-[8px] border border-line bg-panel2'}`}>
                      <span className="mb-0.5 block font-mono text-[10px] text-muted">{m.role === 'user' ? 'you' : agent.name}</span>
                      {m.role === 'user' ? m.content : <Markdown>{m.content}</Markdown>}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div>
              <div className="mb-3 flex items-center justify-between">
                <span className="font-mono text-[11px] tracking-[1px] text-muted">PAST CHATS</span>
              </div>
              {sessions.length === 0 ? null : (
                <div className="grid gap-1.5">
                  {sessions.map((s) => (
                    <button key={s.id} className="flex cursor-pointer items-center justify-between rounded-[10px] border border-line bg-panel2 px-3 py-2 text-left hover:border-mid" onClick={async () => { setViewingSession(s.id); const msgs = await getChatSession(s.id); setHistory(msgs.map((m, i) => ({ role: m.role as 'user' | 'assistant', content: m.content, time: `#${i + 1}` }))); }}>
                      <span className="text-[12.5px] text-text">{s.title}</span>
                      <span className="font-mono text-[11px] text-muted">{fmtDate(s.updatedAt)}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {tab === 'config' && (
        <div className="config-grid mt-4 grid h-[calc(100vh-210px)] min-h-[420px] items-stretch gap-[22px]" style={{ gridTemplateColumns: 'minmax(0, 1fr) 320px' }}>
          <div className="config-main flex min-h-0 flex-col rounded-lg border border-line bg-panel p-[22px]">
            <label className="block flex-none text-[11px] font-semibold text-muted">OBJECTIVE / PROMPT</label>
            <textarea
              value={draft.objective}
              onChange={(e) => setDraft({ ...draft, objective: e.target.value })}
              placeholder="Describe what this agent should do, its role, how it should behave, what format to return…"
              className="prompt-editor mt-[7px] min-h-[300px] w-full flex-1 resize-none rounded-lg border border-line bg-panel2 px-4 py-3.5 font-sans text-[15px] leading-[1.7] text-text outline-none placeholder:text-muted focus:border-mid"
            />
          </div>

          <div className="config-side self-start rounded-lg border border-line bg-panel p-[22px]">
            <label className="block text-[11px] font-semibold text-muted">NAME</label>
            <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} style={{ width: '100%', background: 'var(--panel2)', border: '1px solid var(--line)', borderRadius: 6, padding: '9px 12px', color: 'var(--text)' }} />

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">MODEL</label>
            <Dropdown
              value={draft.model}
              options={models.map((m) => ({ value: m.id, label: m.label }))}
              onChange={(v) => setDraft({ ...draft, model: v })}
            />

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">PERSONA</label>
            <PersonaPicker value={draft.persona ?? 'ai-orb'} onChange={(v) => setDraft({ ...draft, persona: v })} />
            <div className="mt-2 flex items-center gap-2">
              <AgentAvatar agent={{ ...draft, persona: draft.persona ?? 'ai-orb' }} size={28} />
            </div>

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">AVATAR</label>
            <div className="mt-1.5 flex items-center gap-2">
              <input ref={avatarRef} type="file" accept="image/png,image/jpeg,image/gif,image/webp" className="hidden" onChange={onPickAvatar} />
              <button type="button" className="secondary" onClick={() => avatarRef.current?.click()}>Upload image</button>
              {draft.avatar && (
                <button type="button" className="secondary" onClick={() => setDraft({ ...draft, avatar: '' })}>Remove</button>
              )}
            </div>
            <p className="mt-1.5 text-[10.5px] leading-1.5 text-muted">PNG, JPEG, GIF or WebP up to 2 MB. Overrides the persona.</p>

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">TOOLS</label>
            <MultiDropdown
              values={draft.toolIds}
              options={enabledTools.map((t) => ({ value: t.id, label: t.name }))}
              onChange={(v) => setDraft({ ...draft, toolIds: v })}
              placeholder="Select tools…"
            />

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">SKILLS</label>
            <MultiDropdown
              values={draft.skillIds ?? []}
              options={skills.map((s) => ({ value: s.id, label: s.name }))}
              onChange={(v) => setDraft({ ...draft, skillIds: v })}
              placeholder="Select skills…"
            />

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">INTEGRATIONS</label>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              {integrations.filter((i) => i.connected).map((i) => (
                <label key={i.id} style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12 }}>
                  <input type="checkbox" checked={draft.integrations.includes(i.id)} onChange={() => setDraft({ ...draft, integrations: draft.integrations.includes(i.id) ? draft.integrations.filter((x) => x !== i.id) : [...draft.integrations, i.id] })} />
                  {i.name}
                  <span className="font-mono text-[10px] text-muted">({i.actions.length} actions)</span>
                </label>
              ))}
            </div>

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">PERMISSIONS</label>
            <div style={{ display: 'flex', gap: 14, flexWrap: 'wrap' }}>
              {(['network', 'files', 'host_fs'] as const).map((p) => (
                <label key={p} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12 }}>
                  <input type="checkbox" checked={draft.permissions.includes(p)} onChange={() => setDraft({ ...draft, permissions: draft.permissions.includes(p) ? draft.permissions.filter((x) => x !== p) : [...draft.permissions, p] })} />{p}
                </label>
              ))}
            </div>

            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 20 }}>
              <button className="primary" onClick={() => save(true)} disabled={!isComplete(draft)}><Check size={13} />Save &amp; start chatting</button>
            </div>
            {error && <p style={{ fontSize: 11, color: '#f87171', margin: '8px 0 0' }}>{error}</p>}
          </div>
        </div>
      )}
    </div>
  );
}
