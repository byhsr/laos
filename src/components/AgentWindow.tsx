import { useEffect, useRef, useState } from 'react';
import { Check, ChevronRight, Lock, Play } from 'lucide-react';
import type { Agent, ChatMessage, ExecutionResult, Integration, ModelConfig, Run, Tool } from '../types';
import { Dropdown } from './ui/Dropdown';
import { MultiDropdown } from './ui/MultiDropdown';

const isComplete = (a: Agent) => !!a.name.trim() && a.name.trim() !== 'New Agent' && !!a.model.trim() && !!a.objective.trim();

export function AgentWindow({ agent, tools, models, integrations, runs, onBack, onSave, onDelete, onRun }: {
  agent: Agent; tools: Tool[]; models: ModelConfig[]; integrations: Integration[]; runs: Run[];
  onBack: () => void; onSave: (a: Agent) => Promise<void>; onDelete: (id: string) => Promise<void>; onRun: (input: string, agent: Agent) => Promise<ExecutionResult>;
}) {
  const complete = isComplete(agent);
  const [tab, setTab] = useState<'chat' | 'runs' | 'info' | 'config'>(complete ? 'chat' : 'config');
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const [draft, setDraft] = useState<Agent>(agent);
  const toolName = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const enabledTools = tools.filter((t) => t.enabled);
  const scrollRef = useRef<HTMLDivElement>(null);

  const switchTab = (t: 'chat' | 'runs' | 'info' | 'config') => {
    if ((t === 'chat' || t === 'runs') && !complete) { setTab('config'); return; }
    setTab(t);
  };

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [messages]);

  const send = async (text: string) => {
    if (!text.trim() || running) return;
    setRunning(true); setError(undefined);
    const userMsg: ChatMessage = { role: 'user', content: text, time: new Date().toLocaleTimeString() };
    setMessages((prev) => [...prev, userMsg]);
    setInput('');
    const agentToRun = agent;
    try {
      const result = await onRun(text, agentToRun);
      const assistant: ChatMessage = { role: 'assistant', content: result.output, time: new Date().toLocaleTimeString() };
      const toolMsgs: ChatMessage[] = (result.events ?? []).filter((e) => e.type === 'tool').map((e) => ({ role: 'tool' as const, content: e.title, detail: e.detail, time: e.time }));
      setMessages((prev) => [...prev, ...toolMsgs, assistant]);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      setMessages((prev) => [...prev, { role: 'assistant', content: `⚠️ ${message}`, time: new Date().toLocaleTimeString() }]);
    } finally {
      setRunning(false);
    }
  };

  const save = async (startChat = false) => {
    if (!draft.name.trim()) { setError('Agent needs a name.'); return; }
    await onSave(draft);
    setError(undefined);
    if (startChat && isComplete(draft)) setTab('chat');
  };

  const statusColor = (s: string) => s === 'running' ? 'text-[#facc15]' : s === 'completed' ? 'text-[#22c55e]' : 'text-[#f87171]';

  return (
    <div className="boxy">
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        <div>
          <div className="mb-2 flex items-center gap-1.5">
            <button className="cursor-pointer border-0 bg-none p-0 font-mono text-[10px] uppercase tracking-[1px] text-muted hover:text-text" onClick={onBack}>Agents</button>
            <ChevronRight size={12} className="text-[#52525b]" />
            <span className="font-mono text-[10px] uppercase tracking-[1px] text-[var(--purple)]">{agent.name}</span>
          </div>
          <h1 style={{ margin: 0, fontSize: 24 }}>{agent.name}</h1>
        </div>
        <span className="agent-dot inline-block h-2 w-2 rounded-full" style={{ background: agent.color }} />
      </header>

      <div className="flex gap-2 border-b border-line px-3">
        {(['chat', 'runs', 'info', 'config'] as const).map((t) => {
          const locked = !complete && (t === 'chat' || t === 'runs');
          return (
            <button key={t} className={`flex cursor-pointer items-center gap-1 border-0 bg-none px-[5px] py-2.5 text-[11px] capitalize ${tab === t ? 'border-b-2 border-[var(--purple)] text-text' : 'text-muted'}`} onClick={() => switchTab(t)}>
              {t}{locked && <Lock size={9} className="opacity-70" />}
            </button>
          );
        })}
      </div>

      {(tab === 'chat' || tab === 'runs') && !complete && (
        <div className="mt-4 flex h-[calc(100vh-320px)] min-h-[320px] flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-line bg-panel text-center">
          <Lock size={20} className="text-muted" />
          <h3 style={{ margin: 0, fontSize: 14 }}>Finish setting up {agent.name}</h3>
          <p className="text-muted" style={{ margin: 0, maxWidth: 340, fontSize: 12, lineHeight: 1.6 }}>Add a name, pick a model, and write an objective to unlock Chat and Runs.</p>
          <button className="primary" onClick={() => setTab('config')}><Check size={13} />Go to config</button>
        </div>
      )}

      {tab === 'chat' && (
        <div className="chat mt-4 flex h-[calc(100vh-320px)] min-h-[320px] flex-col overflow-hidden rounded-lg border border-line bg-panel">
          <div className="chat-log flex-1 scrollbar-thin scrollbar-color-[#52525b_transparent] overflow-y-auto p-[18px]">
            {messages.length === 0 && (
              <div className="chat-empty m-auto max-w-[360px] text-center text-[13px] leading-[1.6] text-muted">
                <p>Say hello to {agent.name}. Send a task — the agent will use its tools ({agent.toolIds.map(toolName).join(', ') || 'none'}) to get things done.</p>
              </div>
            )}
            {messages.map((m, i) => (
              <div key={i} className={`flex ${m.role === 'user' ? 'justify-end' : m.role === 'tool' ? 'justify-center' : 'justify-start'}`}>
                <div className={`max-w-[78%] rounded-[10px] px-3.5 py-2.5 text-[13.5px] leading-[1.6] whitespace-pre-wrap break-words ${m.role === 'user' ? 'rounded-tr-[3px] bg-[#27272a] text-text' : m.role === 'assistant' ? 'rounded-tl-[3px] border border-line bg-panel2' : 'border-0 bg-transparent p-1 font-mono text-[11px] tracking-[0.3px] text-muted'}`}>
                  {m.role === 'tool' && <span className="opacity-90">⚙ {m.content}{m.detail ? ` — ${m.detail}` : ''}</span>}
                  {(m.role === 'user' || m.role === 'assistant') && <span>{m.content}</span>}
                </div>
              </div>
            ))}
          </div>
          <div className="chat-input-row flex items-end gap-2.5 border-t border-line p-3">
            <textarea
              rows={2} placeholder={`Message ${agent.name}…`}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(input); } }}
              style={{ flex: 1, background: 'var(--panel2)', border: '1px solid var(--line)', borderRadius: 8, padding: '10px 12px', color: 'var(--text)', resize: 'none' }}
            />
            <button className="primary" disabled={running || !input.trim()} onClick={() => send(input)}>
              <Play size={13} />{running ? 'Working…' : 'Send'}
            </button>
          </div>
          {error && <p style={{ fontSize: 11, color: '#f87171', margin: '8px 0 0' }}>{error}</p>}
        </div>
      )}

      {tab === 'runs' && (
        <div className="runs-console mt-4 max-h-[calc(100vh-280px)] overflow-y-auto rounded-lg border border-line bg-[#0a0a0c] p-3.5 font-mono text-[12px] leading-[1.6]">
          {runs.filter((r) => r.agentId === agent.id).length === 0 && (
            <div className="console-empty p-2.5 text-center text-[12px] text-muted"><p>No runs yet for {agent.name}. Send a message in Chat and every step shows up here.</p></div>
          )}
          {runs.filter((r) => r.agentId === agent.id).map((r) => (
            <div key={r.id} className="border-b border-[#1c1c1f] py-3 last:border-0">
              <div className="console-head flex items-center gap-2.5 text-[11px]">
                <span className={`text-muted ${statusColor(r.status)}`}>{r.status === 'running' ? '▸' : r.status === 'completed' ? '✓' : '✕'}</span>
                <span className="text-muted">{r.model}</span>
                <span className="ml-auto text-[#52525b]">{r.startedAt ? new Date(r.startedAt).toLocaleTimeString() : ''}</span>
              </div>
              <div className="my-1.5 text-[#d4d4d8]">$ {r.input}</div>
              {r.events.map((ev, i) => (
                <div key={i} className="console-line flex items-baseline gap-2">
                  <span className="flex-none text-[#52525b]">{ev.time}</span>
                  <span className={`w-10 flex-none text-muted ${ev.type === 'tool' ? 'text-[#38bdf8]' : ev.type === 'thought' ? 'text-[#a78bfa]' : 'text-[#22c55e]'}`}>{ev.type === 'tool' ? 'tool' : ev.type === 'thought' ? 'think' : 'out'}</span>
                  <span className={`text-[#a1a1aa] ${ev.type === 'tool' ? 'text-[#7dd3fc]' : ev.type === 'thought' ? 'text-[#c4b5fd]' : ''}`}>{ev.title}{ev.detail ? ` — ${ev.detail}` : ''}</span>
                </div>
              ))}
              {r.output && <pre className="mt-1.5 ml-12 whitespace-pre-wrap rounded-[6px] border border-[#1c1c1f] bg-[#111113] p-2 text-[11px] text-[#e4e4e7]">{r.output}</pre>}
            </div>
          ))}
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

      {tab === 'config' && (
        <div className="config-grid mt-4 grid items-stretch gap-[18px]" style={{ gridTemplateColumns: 'minmax(0, 1fr) 320px' }}>
          <div className="config-main flex flex-col rounded-lg border border-line bg-panel p-[18px]">
            <label className="block flex-none text-[11px] font-semibold text-muted">OBJECTIVE / PROMPT</label>
            <textarea
              value={draft.objective}
              onChange={(e) => setDraft({ ...draft, objective: e.target.value })}
              placeholder="Describe what this agent should do, its role, how it should behave, what format to return…"
              className="prompt-editor mt-[7px] min-h-[300px] w-full flex-1 resize-none rounded-lg border border-line bg-panel2 px-4 py-3.5 font-sans text-[15px] leading-[1.7] text-text outline-none placeholder:text-muted focus:border-[#52525b]"
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

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">TOOLS</label>
            <MultiDropdown
              values={draft.toolIds}
              options={enabledTools.map((t) => ({ value: t.id, label: t.name }))}
              onChange={(v) => setDraft({ ...draft, toolIds: v })}
              placeholder="Select tools…"
            />
            {enabledTools.length === 0 && <p style={{ fontSize: 11, color: 'var(--muted)', marginTop: 6 }}>No enabled tools. Add some in the Tools tab.</p>}

            <label className="mt-3.5 block text-[11px] font-semibold text-muted">PERMISSIONS</label>
            <div style={{ display: 'flex', gap: 14, flexWrap: 'wrap' }}>
              {(['network', 'files'] as const).map((p) => (
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
