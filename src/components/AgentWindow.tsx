import { useEffect, useRef, useState } from 'react';
import { Check, ChevronRight, Lock, Play, Send } from 'lucide-react';
import type { Agent, ChatMessage, ExecutionResult, Integration, ModelConfig, Run, Tool } from '../types';
import { Dropdown } from './ui/Dropdown';
import { MultiDropdown } from './ui/MultiDropdown';
import { AgentAvatar, PERSONAS } from './ui/AgentAvatar';
import { toast } from '../hooks/useToast';
import { listChatSessions, getChatSession, createChatSession, deleteChatSession, streamChat } from '../runtime';
import { useRunsStore } from '../hooks/useRuns';
import { useManagerStore, type ChatEntry } from '../hooks/useManager';
import { useShallow } from 'zustand/react/shallow';
import { StreamIndicator } from './ui/StreamIndicator';

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

export function AgentWindow({ agent, tools, models, integrations, runs, onBack, onSave, onDelete, onRun }: {
  agent: Agent; tools: Tool[]; models: ModelConfig[]; integrations: Integration[]; runs: Run[];
  onBack: () => void; onSave: (a: Agent) => Promise<void>; onDelete: (id: string) => Promise<void>; onRun: (input: string, agent: Agent) => Promise<ExecutionResult>;
}) {
  const complete = isComplete(agent);
  const [tab, setTab] = useState<'chat' | 'runs' | 'info' | 'config' | 'history'>(complete ? 'chat' : 'config');
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
    let buffer = '';
    try {
      await streamChat(agentToRun, text, false, (delta) => {
        buffer += delta;
        setMessages((prev) => prev.map((m, i) => (i === prev.length - 1 ? { ...m, content: buffer } : m)));
      });
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

  return (
    <div className="boxy flex h-full min-h-0 flex-col overflow-hidden">
      <header className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          <button className="cursor-pointer border-0 bg-none p-0 font-mono text-[10px] uppercase tracking-[1px] text-muted hover:text-text" onClick={onBack}>Agents</button>
          <ChevronRight size={12} className="text-mid" />
          <span className="font-mono text-[10px] uppercase tracking-[1px] text-text">{agent.name}</span>
        </div>
        <div className="flex items-center gap-1">
          {(['chat', 'runs', 'info', 'config', 'history'] as const).map((t) => {
            const locked = !complete && (t === 'chat' || t === 'runs');
            return (
              <button key={t} className={`flex cursor-pointer items-center gap-1 rounded-[6px] border px-2.5 py-1.5 text-[11px] capitalize ${tab === t ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`} onClick={() => switchTab(t)}>
                {t}{locked && <Lock size={9} className="opacity-70" />}
              </button>
            );
          })}
          <span className="agent-dot ml-1 inline-block h-2 w-2 rounded-full" style={{ background: agent.color }} />
          <AgentAvatar agent={agent} size={20} animate={false} />
        </div>
      </header>

      {(tab === 'chat' || tab === 'runs') && !complete && (
        <div className="mt-4 flex h-[calc(100vh-190px)] min-h-[420px] flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-line bg-panel text-center">
          <Lock size={20} className="text-muted" />
          <h3 style={{ margin: 0, fontSize: 14 }}>Finish setting up {agent.name}</h3>
          <p className="text-muted" style={{ margin: 0, maxWidth: 340, fontSize: 12, lineHeight: 1.6 }}>Add a name, pick a model, and write an objective to unlock Chat and Runs.</p>
          <button className="primary" onClick={() => setTab('config')}><Check size={13} />Go to config</button>
        </div>
      )}

      {tab === 'chat' && (
        <div className="relative flex h-[calc(100vh-120px)] min-h-[460px] flex-col">
          {/* Chat history box — full height, input overlays on top of it */}
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-panel">
            <div className="chat-log flex-1 scrollbar-thin scrollbar-color-mid overflow-y-auto px-4 pt-4 pb-24" ref={scrollRef}>
              {messages.length === 0 && (
                <div className="chat-empty m-auto max-w-[360px] text-center text-[13px] leading-[1.6] text-muted">
                  <p>Say hello to {agent.name}. Send a task — the agent will use its tools ({agent.toolIds.map(toolName).join(', ') || 'none'}) to get things done.</p>
                </div>
              )}
              {messages.map((m, i) => (
                <div key={i} className={`mb-3 flex ${m.role === 'user' ? 'justify-end' : 'justify-start'} last:mb-0`}>
                  <div className={`max-w-[78%] rounded-[10px] px-3.5 py-2.5 text-[13.5px] leading-[1.6] whitespace-pre-wrap break-words ${m.role === 'user' ? 'rounded-tr-[3px] bg-line text-text' : 'rounded-tl-[3px] border border-line bg-panel2'}`}>
                    <span>
                      {m.content}
                      {running && i === messages.length - 1 && m.role === 'assistant' && (
                        m.content ? <span className="ml-0.5 inline-block h-3.5 w-[2px] animate-pulse bg-muted align-middle" /> : <StreamIndicator streaming />
                      )}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
          {error && <p className="absolute bottom-[68px] left-4 text-[11px] text-[#f87171]" style={{ margin: 0 }}>{error}</p>}
          {/* Input overlays the chat, floating at the bottom */}
          <div className="absolute right-0 bottom-0 left-0 flex items-end gap-2.5 rounded-lg bg-gradient-to-t from-[var(--bg)] via-[var(--bg)]/80 to-transparent p-3 pt-6">
            <textarea
              ref={inputRef}
              rows={1} placeholder={`Message ${agent.name}…`}
              value={input}
              onChange={onInputChange}
              onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(input); } }}
              style={{ flex: 1, background: 'var(--panel2)', border: '1px solid var(--line)', borderRadius: 10, padding: '12px 14px', color: 'var(--text)', resize: 'none', minHeight: 44, maxHeight: 160, boxShadow: '0 8px 24px #000a' }}
            />
            <button className="primary" disabled={running || !input.trim()} onClick={() => send(input)} title="Send">
              <Send size={14} />
            </button>
          </div>
        </div>
      )}

      {tab === 'runs' && (
        <div className="runs-console mt-4 h-[calc(100vh-190px)] min-h-[420px] overflow-y-auto rounded-lg border border-line bg-[#0a0a0c] p-3.5 font-mono text-[12px] leading-[1.6]">
          {runs.filter((r) => r.agentId === agent.id).length === 0 && (
            <div className="console-empty p-2.5 text-center text-[12px] text-muted"><p>No runs yet for {agent.name}. Send a message in Chat and every step shows up here.</p></div>
          )}
          {runs.filter((r) => r.agentId === agent.id).sort((a, b) => new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()).map((r) => {
            const isOpen = expandedRuns.has(r.id);
            const totalTokens = (r.promptTokens ?? 0) + (r.completionTokens ?? 0);
            return (
              <div key={r.id} className="border-b border-[#1c1c1f] last:border-0">
                <button className="flex w-full cursor-pointer items-center gap-2.5 border-0 bg-transparent px-1 py-2.5 text-left" onClick={() => toggleRun(r.id)}>
                  <ChevronRight size={11} className={`shrink-0 text-mid transition-transform duration-150 ${isOpen ? 'rotate-90' : ''}`} />
                  <span className={`shrink-0 text-muted ${statusColor(r.status)}`}>{r.status === 'running' ? '▸' : r.status === 'completed' ? '✓' : '✕'}</span>
                  <span className="shrink-0 text-mid">{fmtDate(r.startedAt)}</span>
                  <span className="shrink-0 text-muted">{r.model}</span>
                  <span className="shrink-0 text-mid">{runDuration(r)}</span>
                  {totalTokens > 0 && <span className="shrink-0 text-mid">{totalTokens.toLocaleString()} tok</span>}
                  <span className={`ml-auto shrink-0 text-[10px] ${statusColor(r.status)}`}>{r.status}</span>
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
                    {r.output && <pre className="mt-1.5 ml-12 whitespace-pre-wrap rounded-[6px] border border-[#1c1c1f] bg-[#111113] p-2 text-[11px] text-[#e4e4e7]">{r.output}</pre>}
                    {r.status === 'failed' && <div className="console-line flex items-baseline gap-2"><span className="flex-none text-mid" /><span className="w-10 flex-none text-[#22c55e]">err</span><span className="text-[#a1a1aa]">Run failed — see agent chat for details.</span></div>}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {tab === 'info' && (
        <div className="home-content mt-4 rounded-lg border border-line bg-panel p-[18px]">
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold first:mt-0"><span>TOOLS</span></div>
          <div className="flex flex-wrap gap-[5px]">{agent.toolIds.map((t) => <span key={t} className="rounded bg-panel2 px-[7px] py-[5px] text-[10px]">{toolName(t)}</span>)}</div>
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold"><span>INTEGRATIONS</span></div>
          <div className="flex flex-wrap gap-[5px]">{agent.integrations.map((t) => <span key={t} className="rounded bg-panel2 px-[7px] py-[5px] text-[10px]">{t}</span>)}</div>
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold"><span>PERMISSIONS</span></div>
          <div className="mt-2 flex items-center gap-2.5 rounded-[6px] bg-panel2 p-2.5 text-[10px]"><span>{agent.permissions.join(', ') || 'none'}</span></div>
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold"><span>HOME</span></div>
          <div className="flex items-center gap-2.5 rounded-[6px] bg-panel2 p-2.5 text-[10px]"><span>{agent.homePath}</span></div>
          <div className="mb-2.5 mt-5 flex justify-between text-[11px] font-bold"><span>MODEL</span></div>
          <div className="flex items-center gap-2.5 rounded-[6px] bg-panel2 p-2.5 text-[10px]"><span>{agent.model}</span></div>
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
                {history.length === 0 ? <p className="text-center text-[12px] text-muted">No messages in this chat.</p> : history.map((m, i) => (
                  <div key={i} className={`flex ${m.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                    <div className={`max-w-[78%] rounded-[10px] px-3 py-2 text-[12.5px] leading-1.6 whitespace-pre-wrap break-words ${m.role === 'user' ? 'rounded-tr-[3px] bg-line text-text' : 'rounded-tl-[3px] border border-line bg-panel2'}`}>
                      <span className="mb-0.5 block font-mono text-[9px] text-muted">{m.role === 'user' ? 'you' : agent.name}</span>
                      {m.content}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div>
              <div className="mb-3 flex items-center justify-between">
                <span className="font-mono text-[10px] tracking-[1px] text-muted">PAST CHATS</span>
                <button className="primary" onClick={async () => { await createChatSession(agent.id, 'Chat'); listChatSessions(agent.id).then(setSessions); }}>New chat</button>
              </div>
              {sessions.length === 0 ? (
                <p className="text-center text-[12px] text-muted">No past chats yet. Start a chat, then it shows up here as a separate session.</p>
              ) : (
                <div className="grid gap-1.5">
                  {sessions.map((s) => (
                    <button key={s.id} className="flex cursor-pointer items-center justify-between rounded-[6px] border border-line bg-panel2 px-3 py-2 text-left hover:border-mid" onClick={async () => { setViewingSession(s.id); const msgs = await getChatSession(s.id); setHistory(msgs.map((m, i) => ({ role: m.role as 'user' | 'assistant', content: m.content, time: `#${i + 1}` }))); }}>
                      <span className="text-[12.5px] text-text">{s.title}</span>
                      <span className="font-mono text-[10px] text-muted">{fmtDate(s.updatedAt)}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {tab === 'config' && (
        <div className="config-grid mt-4 grid h-[calc(100vh-210px)] min-h-[420px] items-stretch gap-[18px]" style={{ gridTemplateColumns: 'minmax(0, 1fr) 320px' }}>
          <div className="config-main flex min-h-0 flex-col rounded-lg border border-line bg-panel p-[18px]">
            <label className="block flex-none text-[11px] font-semibold text-muted">OBJECTIVE / PROMPT</label>
            <textarea
              value={draft.objective}
              onChange={(e) => setDraft({ ...draft, objective: e.target.value })}
              placeholder="Describe what this agent should do, its role, how it should behave, what format to return…"
              className="prompt-editor mt-[7px] min-h-[300px] w-full flex-1 resize-none rounded-lg border border-line bg-panel2 px-4 py-3.5 font-sans text-[15px] leading-[1.7] text-text outline-none placeholder:text-muted focus:border-mid"
            />
            <p className="config-hint mt-3 flex-none text-[12px] leading-[1.55] text-muted">This becomes the agent's system prompt. It runs on every task, so be specific about role, tone, and output format.</p>
          </div>

          <div className="config-side self-start rounded-lg border border-line bg-panel p-[18px]">
            <label className="block text-[11px] font-semibold text-muted">NAME</label>
            <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} style={{ width: '100%', background: 'var(--panel2)', border: '1px solid var(--line)', borderRadius: 6, padding: '9px 12px', color: 'var(--text)' }} />

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">MODEL</label>
            <Dropdown
              value={draft.model}
              options={models.map((m) => ({ value: m.id, label: m.label }))}
              onChange={(v) => setDraft({ ...draft, model: v })}
            />

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">PERSONA</label>
            <Dropdown
              value={draft.persona ?? 'gremlin'}
              options={PERSONAS.map((p) => ({ value: p.id, label: p.label }))}
              onChange={(v) => setDraft({ ...draft, persona: v })}
              placeholder="Select persona…"
            />
            <div className="mt-2 flex items-center gap-2">
              <AgentAvatar agent={{ ...draft, persona: draft.persona ?? 'gremlin' }} size={28} />
              <span className="text-[11px] text-muted">Animated Lottie gremlin avatar</span>
            </div>

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">TOOLS</label>
            <MultiDropdown
              values={draft.toolIds}
              options={enabledTools.map((t) => ({ value: t.id, label: t.name }))}
              onChange={(v) => setDraft({ ...draft, toolIds: v })}
              placeholder="Select tools…"
            />
            {enabledTools.length === 0 && <p style={{ fontSize: 11, color: 'var(--muted)', marginTop: 6 }}>No enabled tools. Add some in the Tools tab.</p>}

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">INTEGRATIONS</label>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              {integrations.filter((i) => i.connected).map((i) => (
                <label key={i.id} style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12 }}>
                  <input type="checkbox" checked={draft.integrations.includes(i.id)} onChange={() => setDraft({ ...draft, integrations: draft.integrations.includes(i.id) ? draft.integrations.filter((x) => x !== i.id) : [...draft.integrations, i.id] })} />
                  {i.name}
                  <span className="font-mono text-[9px] text-muted">({i.actions.length} actions)</span>
                </label>
              ))}
              {integrations.filter((i) => i.connected).length === 0 && <p style={{ fontSize: 11, color: 'var(--muted)', margin: 0 }}>No connected integrations. Connect them in the Integrations tab.</p>}
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
              <button className="secondary" onClick={() => onDelete(agent.id)}>Delete</button>
              <button className="primary" onClick={() => save(true)} disabled={!isComplete(draft)}><Check size={13} />Save &amp; start chatting</button>
            </div>
            {error && <p style={{ fontSize: 11, color: '#f87171', margin: '8px 0 0' }}>{error}</p>}
          </div>
        </div>
      )}
    </div>
  );
}
