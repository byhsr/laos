import { useCallback, useEffect, useRef, useState } from 'react';
import { ChevronRight, Download, RefreshCw, Search, Upload, X } from 'lucide-react';
import { useAgentsStore } from '../../hooks/useAgents';
import type { BrainObservation } from '../../types';
import { amListMemories, amSearch, amGetSelfModel, amStats, amListNamespaces, amAddMemory, amSetSelfModel, brainListObservations, brainAcceptObservation, brainDismissObservation, brainCorrectObservation, brainReflectNow, brainEval, type BrainEval } from '../../runtime';
import { toast } from '../../hooks/useToast';
import { Select } from '../ui/Select';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { tabCls } from '../ui/tabs';
import { GROUP_LABEL_CLS } from '../ui/Input';

// Memory is native now (agent_memory.rs in the backend); this view reads it
// through the am_* commands.

type FoxMemory = {
  id: string; agentId: string; scope: string; kind: string; status: string;
  title: string; content: string; tags?: string[];
  confidence?: number; importance?: number; updatedAt?: number;
};
type FoxSelfEntry = { id: string; key: string; value: string; confidence?: number; importance?: number; updatedAt?: number };
type FoxStats = { events: number; memories: number; selfModelEntries: number };
type FoxNamespace = { namespace: { key: string; type: string; id: string | null; name: string; description?: string | null }; counts: { events: number; memories: number; selfModelEntries: number }; lastActivityAt?: number | null };

type Tab = 'memories' | 'self' | 'namespaces' | 'observations';

const DIMENSION_LABEL: Record<string, string> = {
  salience: 'salience', allocation: 'attention', intention: 'intention',
  displacement: 'displaced', persistence: 'persistence', drift: 'drift',
  momentum: 'momentum', dormancy: 'dormant', reentry: 're-entry', pattern: 'pattern',
  entity_resolution: 'resolution',
};

const fmtDate = (ms?: number | null) => {
  if (!ms) return '';
  const d = new Date(ms);
  return isNaN(d.getTime()) ? '' : d.toLocaleString();
};
const pct = (n?: number) => (typeof n === 'number' ? `${Math.round(n * 100)}%` : '');

// A memory's scope, collapsed to a short badge. An agent's memory can now live in
// its own scope, a project scope it chatted in, or global.
const scopeBadge = (s: string) => (s === 'global' ? 'global' : s.startsWith('project:') ? 'project' : 'agent');

export function MemoryView() {
  const agents = useAgentsStore((s) => s.agents);
  const [tab, setTab] = useState<Tab>('memories');
  const [agentId, setAgentId] = useState('');
  const [query, setQuery] = useState('');
  const [submitted, setSubmitted] = useState('');
  const [memories, setMemories] = useState<FoxMemory[]>([]);
  const [selfEntries, setSelfEntries] = useState<FoxSelfEntry[]>([]);
  const [namespaces, setNamespaces] = useState<FoxNamespace[]>([]);
  const [stats, setStats] = useState<FoxStats | null>(null);
  const [observations, setObservations] = useState<BrainObservation[]>([]);
  const [evalr, setEvalr] = useState<BrainEval | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);
  const [editId, setEditId] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [memScope, setMemScope] = useState<'all' | 'agent' | 'project' | 'global'>('all');

  // Default to the lead agent so the view has something to show immediately.
  useEffect(() => {
    if (agentId || agents.length === 0) return;
    setAgentId((agents.find((a) => a.isManager) ?? agents[0]).id);
  }, [agents, agentId]);

  const load = useCallback(async () => {
    if (!agentId) return;
    setLoading(true);
    setError(null);
    try {
      const [mem, self, st] = await Promise.all([
        submitted ? amSearch(agentId, submitted, undefined, 30) : amListMemories(agentId),
        amGetSelfModel(agentId),
        amStats(),
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

  // Namespaces = tracked scopes (projects). Loaded on demand from the connector.
  const loadNamespaces = useCallback(async () => {
    try { setNamespaces(((await amListNamespaces()) as FoxNamespace[]) ?? []); }
    catch { setNamespaces([]); }
  }, []);
  useEffect(() => { if (tab === 'namespaces') void loadNamespaces(); }, [tab, loadNamespaces]);

  // Inferred observations (the attention / learning layer), loaded on demand.
  const loadObservations = useCallback(async () => {
    try { setObservations((await brainListObservations()) ?? []); }
    catch { setObservations([]); }
  }, []);
  useEffect(() => {
    if (tab !== 'observations') return;
    void loadObservations();
    void brainEval().then(setEvalr);
  }, [tab, loadObservations]);

  const reflect = async () => {
    setBusy(true);
    try {
      const n = await brainReflectNow();
      toast(n > 0 ? `${n} observation${n === 1 ? '' : 's'}` : 'nothing new to infer', 'success');
      await loadObservations();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const act = async (id: string, kind: 'accept' | 'dismiss') => {
    if (kind === 'accept') await brainAcceptObservation(id); else await brainDismissObservation(id);
    await loadObservations();
  };

  const saveCorrection = async (id: string) => {
    await brainCorrectObservation(id, { body: draft });
    setEditId(null);
    await loadObservations();
  };

  const fileRef = useRef<HTMLInputElement>(null);

  // Export/import the agent's durable memory (memories + self-model) as JSON, so
  // it can be moved between agents/machines.
  const exportMemory = async () => {
    if (!agentId) return;
    try {
      const [mem, self] = await Promise.all([
        amListMemories(agentId),
        amGetSelfModel(agentId),
      ]);
      const doc = { version: 1, kind: 'lup-memory', agentId, exportedAt: new Date().toISOString(), memories: mem, selfModel: self };
      const blob = new Blob([JSON.stringify(doc, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url; a.download = `memory-${agentId}.json`; a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const importMemory = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file || !agentId) return;
    const scope = `agent:${agentId}`;
    try {
      const doc = JSON.parse(await file.text());
      const memories: FoxMemory[] = Array.isArray(doc?.memories) ? doc.memories : [];
      const self: FoxSelfEntry[] = Array.isArray(doc?.selfModel) ? doc.selfModel : [];
      let n = 0;
      for (const m of memories) {
        await amAddMemory({
          agentId, scope, kind: m.kind ?? 'semantic', title: m.title ?? 'imported', content: m.content ?? '',
          ...(Array.isArray(m.tags) ? { tags: m.tags } : {}), ...(m.importance !== undefined ? { importance: m.importance } : {}), ...(m.confidence !== undefined ? { confidence: m.confidence } : {}),
        });
        n++;
      }
      for (const s of self) {
        if (!s?.key || !s?.value) continue;
        await amSetSelfModel(agentId, null, s.key, s.value);
        n++;
      }
      toast(`imported ${n} item${n === 1 ? '' : 's'}`, 'success');
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const agentOptions = agents
    .slice()
    .sort((a, b) => Number(!!b.isManager) - Number(!!a.isManager) || a.name.localeCompare(b.name))
    .map((a) => ({ value: a.id, label: a.name }));

  const visibleMemories = memScope === 'all' ? memories : memories.filter((m) => scopeBadge(m.scope) === memScope);

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
          {tab === 'memories' && (
            <div className="w-[130px]">
              <Select
                value={memScope}
                options={[
                  { value: 'all', label: 'all scopes' },
                  { value: 'agent', label: 'this agent' },
                  { value: 'project', label: 'projects' },
                  { value: 'global', label: 'global' },
                ]}
                onChange={(v) => setMemScope(v as typeof memScope)}
                placeholder="all scopes"
              />
            </div>
          )}
          <IconButton label="export memory" className="h-[30px] w-[30px] shrink-0" onClick={() => void exportMemory()}><Download size={13} /></IconButton>
          <IconButton label="import memory" className="h-[30px] w-[30px] shrink-0" onClick={() => fileRef.current?.click()}><Upload size={13} /></IconButton>
          <input ref={fileRef} type="file" accept="application/json,.json" className="hidden" onChange={(e) => void importMemory(e)} />
          {([['memories', 'memories'], ['self', 'self-model'], ['namespaces', 'namespaces'], ['observations', 'mind']] as const).map(([key, label]) => (
            <button key={key} className={tabCls(tab === key)} onClick={() => setTab(key)}>{label}</button>
          ))}
        </div>
      </div>

      {/* Global counts — the whole store, across agents. */}
      {stats && (
        <div className="mb-3 shrink-0 font-mono text-[10px] text-muted">
          {(stats.events ?? 0).toLocaleString()} events · {(stats.memories ?? 0).toLocaleString()} memories · {(stats.selfModelEntries ?? 0).toLocaleString()} self-model
        </div>
      )}

      {error && (
        <Card className="mb-3 shrink-0">
          <p className="m-0 font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap text-danger">{error}</p>
        </Card>
      )}

      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'memories' ? '' : 'hidden'}`}>
        {visibleMemories.length === 0 ? (
          <p className="m-0 px-1 font-mono text-[11px] text-muted">
            {loading ? 'loading…' : submitted ? 'no matches' : memScope !== 'all' && memories.length > 0 ? 'no memories in this scope' : 'no memories yet — memory from chats in a project is listed here too'}
          </p>
        ) : (
          <div className="grid gap-2">
            {visibleMemories.map((m) => {
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
                    <span title={m.scope} className="shrink-0 rounded bg-background px-1.5 py-0.5 font-mono text-[9.5px] text-muted">{scopeBadge(m.scope)}</span>
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
      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'namespaces' ? '' : 'hidden'}`}>
        {namespaces.length === 0 ? (
          <p className="m-0 px-1 font-mono text-[11px] text-muted">no namespaces yet — a project registers one, and turns in it file their memory there</p>
        ) : (
          <div className="grid gap-2">
            {namespaces.map((n) => (
              <div key={n.namespace.key} className="rounded-xl border border-border bg-surface px-3.5 py-2.5">
                <div className="flex items-center gap-2">
                  <span className="shrink-0 rounded bg-background px-1.5 py-0.5 font-mono text-[9.5px] text-muted">{n.namespace.type}</span>
                  <b className="min-w-0 flex-1 truncate font-mono text-[12px] text-foreground">{n.namespace.name}</b>
                  {fmtDate(n.lastActivityAt) && <span className="shrink-0 font-mono text-[10px] text-muted">{fmtDate(n.lastActivityAt)}</span>}
                </div>
                {n.namespace.description && <p className="mt-1 mb-0 font-mono text-[10px] text-muted">{n.namespace.description}</p>}
                <div className="mt-1 font-mono text-[10px] text-muted">
                  {(n.counts?.memories ?? 0).toLocaleString()} memories · {(n.counts?.events ?? 0).toLocaleString()} events · {(n.counts?.selfModelEntries ?? 0).toLocaleString()} self-model
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'observations' ? '' : 'hidden'}`}>
        <div className="mb-2 flex items-center justify-between gap-2">
          <p className="m-0 font-mono text-[10px] leading-relaxed text-muted">
            Patterns inferred from what you actually file — proposals, never facts. Accept, correct or dismiss them.
          </p>
          <IconButton label="reflect now" className="h-[26px] w-[26px] shrink-0" onClick={() => void reflect()}>
            <RefreshCw size={12} className={busy ? 'animate-spin' : ''} />
          </IconButton>
        </div>
        {observations.length === 0 ? (
          <p className="m-0 px-1 font-mono text-[11px] text-muted">nothing inferred yet</p>
        ) : (
          <div className="grid gap-2">
            {observations.map((o) => (
              <div key={o.id} className="rounded-xl border border-border bg-surface px-3.5 py-2.5">
                <div className="flex items-center gap-2">
                  <span className="shrink-0 rounded bg-background px-1.5 py-0.5 font-mono text-[9.5px] text-muted">{DIMENSION_LABEL[o.dimension] ?? o.dimension}</span>
                  <b className="min-w-0 flex-1 truncate text-[12.5px] text-foreground">{o.title}</b>
                  <span className="shrink-0 font-mono text-[10px] text-muted">{o.status} · conf {pct(o.confidence)}</span>
                </div>
                {editId === o.id ? (
                  <>
                    <textarea value={draft} onChange={(e) => setDraft(e.target.value)} rows={2} className="mt-2 w-full rounded-lg border border-border bg-background px-2 py-1 font-mono text-[11px] text-foreground outline-none" />
                    <div className="mt-1.5 flex gap-1.5">
                      <Button onClick={() => void saveCorrection(o.id)}>save</Button>
                      <Button onClick={() => setEditId(null)}>cancel</Button>
                    </div>
                  </>
                ) : (
                  <>
                    <p className="mt-1.5 mb-0 text-[12.5px] leading-[1.6] break-words whitespace-pre-wrap text-foreground/80">{o.body}</p>
                    <div className="mt-2 flex gap-1.5">
                      {o.status === 'new' && <Button onClick={() => void act(o.id, 'accept')}>accept</Button>}
                      <Button onClick={() => { setEditId(o.id); setDraft(o.body); }}>correct</Button>
                      {o.status !== 'dismissed' && <Button onClick={() => void act(o.id, 'dismiss')}>dismiss</Button>}
                    </div>
                  </>
                )}
              </div>
            ))}
          </div>
        )}
        {evalr && (
          <p className="mt-3 mb-0 font-mono text-[10px] leading-relaxed text-muted">
            eval · precision {pct(evalr.quality.precision)} · recall {pct(evalr.quality.recall)} · mrr {evalr.quality.mrr.toFixed(2)}
            {' · '}{evalr.quality.cases} cases · dup {evalr.quality.reliability.duplicates.ok ? 'ok' : 'FAIL'}
            {' · '}extract {evalr.quality.reliability.extraction.ok ? 'ok' : 'FAIL'}
            <br />
            live · {evalr.live.nodes} nodes · {evalr.live.vector_scan.vectors} vectors · scan {evalr.live.vector_scan.micros}µs
            {' · '}{evalr.live.fts ? 'fts5' : 'lexical scan'} · {evalr.live.guidance}
          </p>
        )}
      </div>
    </div>
  );
}
