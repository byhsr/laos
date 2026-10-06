import { useCallback, useEffect, useState } from 'react';
import { ChevronRight, RefreshCw, Search, X } from 'lucide-react';
import { useAgentsStore } from '../../hooks/useAgents';
import { callMcpTool } from '../../runtime';
import { Select } from '../ui/Select';
import { IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { tabCls } from '../ui/tabs';
import { GROUP_LABEL_CLS } from '../ui/Input';

// The memory engine is the built-in "memory" MCP server (fox / agent-memory),
// seeded on startup. This view reads it directly through call_mcp_tool.
const MEMORY_SERVER = 'memory';

type FoxMemory = {
  id: string; agentId: string; scope: string; kind: string; status: string;
  title: string; content: string; tags?: string[];
  confidence?: number; importance?: number; updatedAt?: number;
};
type FoxSelfEntry = { id: string; key: string; value: string; confidence?: number; importance?: number; updatedAt?: number };
type FoxStats = { events: number; memories: number; selfModelEntries: number };

type Tab = 'memories' | 'self';

const fmtDate = (ms?: number | null) => {
  if (!ms) return '';
  const d = new Date(ms);
  return isNaN(d.getTime()) ? '' : d.toLocaleString();
};
const pct = (n?: number) => (typeof n === 'number' ? `${Math.round(n * 100)}%` : '');

export function MemoryView() {
  const agents = useAgentsStore((s) => s.agents);
  const [tab, setTab] = useState<Tab>('memories');
  const [agentId, setAgentId] = useState('');
  const [query, setQuery] = useState('');
  const [submitted, setSubmitted] = useState('');
  const [memories, setMemories] = useState<FoxMemory[]>([]);
  const [selfEntries, setSelfEntries] = useState<FoxSelfEntry[]>([]);
  const [stats, setStats] = useState<FoxStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);

  // Default to the lead agent so the view has something to show immediately.
  useEffect(() => {
    if (agentId || agents.length === 0) return;
    setAgentId((agents.find((a) => a.isManager) ?? agents[0]).id);
  }, [agents, agentId]);

  const load = useCallback(async () => {
    if (!agentId) return;
    setLoading(true);
    setError(null);
    const scope = { type: 'agent', id: agentId };
    try {
      const [mem, self, st] = await Promise.all([
        submitted
          ? callMcpTool(MEMORY_SERVER, 'search', { agentId, scope, scopeMode: 'inherit', query: submitted, limit: 30 })
          : callMcpTool(MEMORY_SERVER, 'list_memories', { agentId, scope, scopeMode: 'inherit' }),
        callMcpTool(MEMORY_SERVER, 'get_self_model', { agentId }),
        callMcpTool(MEMORY_SERVER, 'stats', {}),
      ]);
      // `search` returns RetrievalHit[] (wrapped); `list_memories` returns Memory[].
      const list = submitted
        ? ((mem as { memory: FoxMemory }[]) ?? []).map((h) => h.memory)
        : ((mem as FoxMemory[]) ?? []);
      setMemories(list);
      setSelfEntries((self as FoxSelfEntry[]) ?? []);
      setStats((st as FoxStats) ?? null);
      setOpenId(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setMemories([]);
      setSelfEntries([]);
    } finally {
      setLoading(false);
    }
  }, [agentId, submitted]);

  useEffect(() => { void load(); }, [load]);

  const agentOptions = agents
    .slice()
    .sort((a, b) => Number(!!b.isManager) - Number(!!a.isManager) || a.name.localeCompare(b.name))
    .map((a) => ({ value: a.id, label: a.name }));

  return (
    <div className="flex h-full flex-col">
      {/* Header: whose memory, what to search, which facet. */}
      <div className="mb-3 flex shrink-0 flex-wrap items-center gap-2">
        <div className="min-w-[180px] max-w-[260px]">
          <Select value={agentId} options={agentOptions} onChange={setAgentId} placeholder="select agent…" />
        </div>

        <div className="flex min-w-[200px] flex-1 items-center gap-1.5">
          <div className="relative flex-1">
            <Search size={12} className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-muted" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') setSubmitted(query.trim()); }}
              placeholder="search memories…"
              className="h-[30px] w-full rounded-lg border border-border bg-surface pr-7 pl-7 font-mono text-[11px] text-foreground outline-none transition-colors placeholder:text-muted/50 focus:border-foreground/40"
            />
            {query && (
              <button
                type="button"
                aria-label="clear search"
                className="absolute top-1/2 right-1.5 -translate-y-1/2 cursor-pointer text-muted transition-colors hover:text-foreground"
                onClick={() => { setQuery(''); setSubmitted(''); }}
              >
                <X size={12} />
              </button>
            )}
          </div>
          <IconButton label="refresh" className="h-[30px] w-[30px] shrink-0" onClick={() => void load()}>
            <RefreshCw size={13} className={loading ? 'animate-spin' : ''} />
          </IconButton>
        </div>

        <div className="ml-auto flex items-center gap-1">
          {([['memories', 'memories'], ['self', 'self-model']] as const).map(([key, label]) => (
            <button key={key} className={tabCls(tab === key)} onClick={() => setTab(key)}>{label}</button>
          ))}
        </div>
      </div>

      {/* Global counts — the whole store, across agents. */}
      {stats && (
        <div className="mb-3 shrink-0 font-mono text-[10px] text-muted">
          {stats.events.toLocaleString()} events · {stats.memories.toLocaleString()} memories · {stats.selfModelEntries.toLocaleString()} self-model
        </div>
      )}

      {error && (
        <Card className="mb-3 shrink-0">
          <p className="m-0 font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap text-danger">{error}</p>
        </Card>
      )}

      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'memories' ? '' : 'hidden'}`}>
        {memories.length === 0 ? (
          <p className="m-0 px-1 font-mono text-[11px] text-muted">
            {loading ? 'loading…' : submitted ? 'no matches' : 'no memories yet — ask an agent to remember something'}
          </p>
        ) : (
          <div className="grid gap-2">
            {memories.map((m) => {
              const open = openId === m.id;
              return (
                <button
                  key={m.id}
                  type="button"
                  className="focus-ring w-full cursor-pointer rounded-xl border border-border bg-surface px-3.5 py-2.5 text-left transition-colors hover:border-foreground/30"
                  onClick={() => setOpenId(open ? null : m.id)}
                >
                  <div className="flex items-center gap-2">
                    <ChevronRight size={12} className={`shrink-0 text-muted transition-transform duration-150 ${open ? 'rotate-90' : ''}`} />
                    <span className="min-w-0 flex-1 truncate text-[13px] text-foreground">{m.title}</span>
                    <span className="shrink-0 rounded bg-background px-1.5 py-0.5 font-mono text-[9.5px] text-muted">{m.kind}</span>
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-x-2.5 gap-y-1 pl-5 font-mono text-[10px] text-muted">
                    {m.tags?.length ? <span className="min-w-0 truncate">{m.tags.map((t) => `#${t}`).join(' ')}</span> : null}
                    {m.confidence !== undefined ? <span>conf {pct(m.confidence)}</span> : null}
                    {m.importance !== undefined ? <span>imp {pct(m.importance)}</span> : null}
                    {fmtDate(m.updatedAt) && <span>{fmtDate(m.updatedAt)}</span>}
                  </div>
                  {open && (
                    <p className="mt-2 mb-0 pl-5 text-[12.5px] leading-[1.6] break-words whitespace-pre-wrap text-foreground/80">{m.content}</p>
                  )}
                </button>
              );
            })}
          </div>
        )}
      </div>

      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'self' ? '' : 'hidden'}`}>
        {selfEntries.length === 0 ? (
          <p className="m-0 px-1 font-mono text-[11px] text-muted">{loading ? 'loading…' : 'no self-model entries'}</p>
        ) : (
          <div className="grid gap-2">
            {selfEntries.map((s) => (
              <div key={s.id} className="rounded-xl border border-border bg-surface px-3.5 py-2.5">
                <div className="mb-1 flex items-center gap-2">
                  <span className={GROUP_LABEL_CLS}>{s.key}</span>
                  {s.importance !== undefined && <span className="ml-auto font-mono text-[10px] text-muted">imp {pct(s.importance)}</span>}
                </div>
                <p className="m-0 text-[12.5px] leading-[1.6] break-words whitespace-pre-wrap text-foreground/80">{s.value}</p>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
