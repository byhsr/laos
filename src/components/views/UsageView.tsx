import { useMemo, useState } from 'react';
import { ChartColumn } from 'lucide-react';
import type { Agent, Run } from '../../types';
import { fmtCompact, fmtFull } from '../../format';
import { GROUP_LABEL_CLS } from '../ui/Input';
import { Select } from '../ui/Select';
import { Tooltip } from '../ui/Tooltip';
import { tabCls } from '../ui/tabs';

type Range = '7d' | '30d' | '90d' | 'all';
type Metric = 'total' | 'in' | 'out';

const DAY = 86_400_000;
const RANGE_DAYS: Record<Exclude<Range, 'all'>, number> = { '7d': 7, '30d': 30, '90d': 90 };
// The daily chart never draws more bars than this; a longer span is bucketed.
const MAX_BARS = 92;

const RANGES: { value: Range; label: string }[] = [
  { value: '7d', label: '7d' },
  { value: '30d', label: '30d' },
  { value: '90d', label: '90d' },
  { value: 'all', label: 'all' },
];
const METRICS: { value: Metric; label: string }[] = [
  { value: 'total', label: 'total' },
  { value: 'in', label: 'in' },
  { value: 'out', label: 'out' },
];

const dayStart = (ms: number) => { const d = new Date(ms); d.setHours(0, 0, 0, 0); return d.getTime(); };
const shortDay = (ms: number) => new Date(ms).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });

type Bar = { start: number; label: string; inn: number; out: number };

// Full-view token usage: one filter set (range · agent · model · metric) drives a
// daily time series and the by-agent / by-model breakdowns beneath it.
export function UsageView({ runs, agents, onOpenAgent }: {
  runs: Run[]; agents: Agent[]; onOpenAgent: (id: string) => void;
}) {
  const [range, setRange] = useState<Range>('30d');
  const [metric, setMetric] = useState<Metric>('total');
  const [agentId, setAgentId] = useState('all');
  const [model, setModel] = useState('all');
  const [hover, setHover] = useState<number | null>(null);

  const agentName = (id: string) => agents.find((a) => a.id === id)?.name ?? id;

  const models = useMemo(() => {
    const set = new Set<string>();
    for (const r of runs) if (r.model) set.add(r.model);
    return [...set].sort();
  }, [runs]);

  // Apply the range / agent / model filters. The metric only decides what the
  // bars measure, so it is applied further down.
  const filtered = useMemo(() => {
    const since = range === 'all' ? 0 : dayStart(Date.now()) - (RANGE_DAYS[range] - 1) * DAY;
    return runs.filter((r) => {
      if (agentId !== 'all' && r.agentId !== agentId) return false;
      if (model !== 'all' && r.model !== model) return false;
      if (since) { const t = new Date(r.startedAt).getTime(); if (isNaN(t) || t < since) return false; }
      return true;
    });
  }, [runs, range, agentId, model]);

  const totalIn = filtered.reduce((s, r) => s + (r.promptTokens ?? 0), 0);
  const totalOut = filtered.reduce((s, r) => s + (r.completionTokens ?? 0), 0);
  const total = totalIn + totalOut;
  const avg = filtered.length ? Math.round(total / filtered.length) : 0;

  // The bar value for the current metric — 'total' is in + out.
  const value = (inn: number, out: number) => (metric === 'out' ? out : metric === 'in' ? inn : inn + out);

  // Daily series: every day in the window, from the earliest run when the range
  // is 'all'. Auto-bucketed when the span would draw too many bars.
  const { bars, spanDays } = useMemo(() => {
    const today = dayStart(Date.now());
    let start: number;
    if (range === 'all') {
      const times = filtered.map((r) => new Date(r.startedAt).getTime()).filter((t) => !isNaN(t));
      start = times.length ? dayStart(Math.min(...times)) : today;
    } else {
      start = today - (RANGE_DAYS[range] - 1) * DAY;
    }
    const span = Math.max(1, Math.round((today - start) / DAY) + 1);
    const bucketDays = Math.max(1, Math.ceil(span / MAX_BARS));
    const bucketMs = bucketDays * DAY;
    const out: Bar[] = [];
    for (let s = start; s <= today; s += bucketMs) out.push({ start: s, label: shortDay(s), inn: 0, out: 0 });
    for (const r of filtered) {
      const t = new Date(r.startedAt).getTime();
      if (isNaN(t) || t < start) continue;
      const i = Math.min(out.length - 1, Math.floor((t - start) / bucketMs));
      out[i].inn += r.promptTokens ?? 0;
      out[i].out += r.completionTokens ?? 0;
    }
    return { bars: out, spanDays: bucketDays };
  }, [filtered, range]);

  const maxBar = Math.max(1, ...bars.map((b) => value(b.inn, b.out)));

  // Breakdowns, grouped and sorted by the selected metric.
  const group = (keyOf: (r: Run) => string) => {
    const map = new Map<string, { inn: number; out: number; runs: number }>();
    for (const r of filtered) {
      const k = keyOf(r);
      const g = map.get(k) ?? { inn: 0, out: 0, runs: 0 };
      g.inn += r.promptTokens ?? 0; g.out += r.completionTokens ?? 0; g.runs += 1;
      map.set(k, g);
    }
    return [...map.entries()]
      .map(([key, g]) => ({ key, ...g, v: value(g.inn, g.out) }))
      .filter((g) => g.v > 0)
      .sort((a, b) => b.v - a.v);
  };
  const byAgent = useMemo(() => group((r) => r.agentId), [filtered, metric]); // eslint-disable-line react-hooks/exhaustive-deps
  const byModel = useMemo(() => group((r) => r.model || 'unknown'), [filtered, metric]); // eslint-disable-line react-hooks/exhaustive-deps
  const maxAgent = Math.max(1, ...byAgent.map((g) => g.v));
  const maxModel = Math.max(1, ...byModel.map((g) => g.v));

  const showIn = metric !== 'out';
  const showOut = metric !== 'in';
  const hovered = hover != null ? bars[hover] : null;
  const labelStep = Math.max(1, Math.ceil(bars.length / 6));

  const card = 'rounded-xl border border-border bg-surface p-4';
  const big = 'text-[30px] leading-none font-bold tracking-tight text-foreground';
  const sub = 'font-mono text-[10px] text-muted';

  return (
    <div className="flex flex-col gap-4">
      {/* Filters — one row, chrome-dense, driving everything below. */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div className="flex items-center gap-1">
          {RANGES.map((r) => (
            <button key={r.value} className={tabCls(range === r.value)} onClick={() => { setRange(r.value); setHover(null); }}>{r.label}</button>
          ))}
        </div>
        <div className="flex items-center gap-1">
          <span className={`${GROUP_LABEL_CLS} mr-1`}>metric</span>
          {METRICS.map((m) => (
            <button key={m.value} className={tabCls(metric === m.value)} onClick={() => setMetric(m.value)}>{m.label}</button>
          ))}
        </div>
        <div className="ml-auto flex items-center gap-2">
          <div className="w-[170px]">
            <Select
              value={agentId}
              options={[{ value: 'all', label: 'all agents' }, ...agents.map((a) => ({ value: a.id, label: a.name }))]}
              onChange={setAgentId}
            />
          </div>
          <div className="w-[190px]">
            <Select
              value={model}
              options={[{ value: 'all', label: 'all models' }, ...models.map((m) => ({ value: m, label: m }))]}
              onChange={setModel}
            />
          </div>
        </div>
      </div>

      {/* Summary strip. */}
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <div className={card}>
          <span className={big}>{fmtCompact(total)}</span>
          <span className={`${GROUP_LABEL_CLS} mt-2 block`}>tokens</span>
          <span className={`${sub} mt-0.5 block`}>{fmtFull(total)}</span>
        </div>
        <div className={card}>
          <Tooltip label={fmtFull(totalIn)} className="inline-block"><span className={big}>{fmtCompact(totalIn)}</span></Tooltip>
          <span className={`${GROUP_LABEL_CLS} mt-2 block`}>in</span>
          <span className={`${sub} mt-0.5 block`}>{total ? `${Math.round((totalIn / total) * 100)}% of total` : '—'}</span>
        </div>
        <div className={card}>
          <Tooltip label={fmtFull(totalOut)} className="inline-block"><span className={big}>{fmtCompact(totalOut)}</span></Tooltip>
          <span className={`${GROUP_LABEL_CLS} mt-2 block`}>out</span>
          <span className={`${sub} mt-0.5 block`}>{total ? `${Math.round((totalOut / total) * 100)}% of total` : '—'}</span>
        </div>
        <div className={card}>
          <span className={big}>{filtered.length}</span>
          <span className={`${GROUP_LABEL_CLS} mt-2 block`}>runs</span>
          <span className={`${sub} mt-0.5 block`}>{filtered.length ? `${fmtCompact(avg)} avg / run` : '—'}</span>
        </div>
      </div>

      {/* Daily bar chart. */}
      <div className={card}>
        <div className="mb-3 flex items-baseline justify-between gap-3">
          <span className={GROUP_LABEL_CLS}>tokens by {spanDays > 1 ? `${spanDays}-day` : 'day'}</span>
          <span className="min-w-0 truncate font-mono text-[10px] text-muted">
            {hovered
              ? `${hovered.label} · ${fmtCompact(value(hovered.inn, hovered.out))} (${fmtCompact(hovered.inn)} in · ${fmtCompact(hovered.out)} out)`
              : `peak ${fmtCompact(maxBar)}`}
          </span>
        </div>

        {bars.length === 0 ? (
          <p className="m-0 py-10 text-center font-mono text-[11px] text-muted">no runs in this range</p>
        ) : (
          <>
            <div className="flex h-[160px] w-full items-end gap-px overflow-x-hidden" onMouseLeave={() => setHover(null)}>
              {bars.map((b, i) => {
                const v = value(b.inn, b.out);
                const hOut = (showOut ? b.out : 0) / maxBar * 100;
                const hIn = (showIn ? b.inn : 0) / maxBar * 100;
                return (
                  <div
                    key={b.start}
                    onMouseEnter={() => setHover(i)}
                    className={`flex h-full min-w-0 flex-1 cursor-default flex-col justify-end ${hover === i ? 'bg-background' : ''}`}
                  >
                    {showOut && hOut > 0 && <div style={{ height: `${hOut}%` }} className="w-full bg-foreground/70" />}
                    {showIn && hIn > 0 && <div style={{ height: `${hIn}%` }} className="w-full bg-foreground/30" />}
                    {v === 0 && <div className="h-px w-full bg-border" />}
                  </div>
                );
              })}
            </div>
            <div className="mt-1.5 flex w-full gap-px overflow-x-hidden">
              {bars.map((b, i) => (
                <span key={b.start} className="min-w-0 flex-1 truncate text-center font-mono text-[9px] text-muted">
                  {i % labelStep === 0 || i === bars.length - 1 ? b.label : ''}
                </span>
              ))}
            </div>
            <div className="mt-3 flex items-center gap-4 border-t border-border pt-2.5">
              {showIn && (
                <span className="flex items-center gap-1.5 font-mono text-[10px] text-muted">
                  <i className="inline-block h-2 w-2 rounded-sm bg-foreground/30" />in
                </span>
              )}
              {showOut && (
                <span className="flex items-center gap-1.5 font-mono text-[10px] text-muted">
                  <i className="inline-block h-2 w-2 rounded-sm bg-foreground/70" />out
                </span>
              )}
            </div>
          </>
        )}
      </div>

      {/* Breakdowns. */}
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        <Breakdown
          title="by agent"
          rows={byAgent.map((g) => ({ key: g.key, label: agentName(g.key), v: g.v, runs: g.runs }))}
          max={maxAgent}
          onSelect={(key) => onOpenAgent(key)}
        />
        <Breakdown
          title="by model"
          rows={byModel.map((g) => ({ key: g.key, label: g.key, v: g.v, runs: g.runs }))}
          max={maxModel}
        />
      </div>
    </div>
  );
}

// A ranked horizontal bar list — the same grammar for every grouping.
function Breakdown({ title, rows, max, onSelect }: {
  title: string;
  rows: { key: string; label: string; v: number; runs: number }[];
  max: number;
  onSelect?: (key: string) => void;
}) {
  return (
    <div className="rounded-xl border border-border bg-surface p-4">
      <span className={GROUP_LABEL_CLS}>{title}</span>
      {rows.length === 0 ? (
        <p className="m-0 px-1 py-6 text-center font-mono text-[11px] text-muted">nothing recorded</p>
      ) : (
        <div className="mt-2 flex flex-col gap-2">
          {rows.map((r) => {
            const row = (
              <>
                <div className="flex items-baseline justify-between gap-2">
                  <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{r.label}</span>
                  <span className="shrink-0 font-mono text-[10px] text-muted">{r.runs} run{r.runs === 1 ? '' : 's'}</span>
                  <Tooltip label={fmtFull(r.v)} side="right" className="inline-flex w-16 shrink-0 justify-end">
                    <span className="font-mono text-[11px] text-foreground">{fmtCompact(r.v)}</span>
                  </Tooltip>
                </div>
                <div className="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-background">
                  <div className="h-full rounded-full bg-foreground/50" style={{ width: `${Math.max(2, (r.v / max) * 100)}%` }} />
                </div>
              </>
            );
            return onSelect ? (
              <button key={r.key} type="button" className="focus-ring block w-full cursor-pointer rounded border-0 bg-transparent p-0 text-left transition-colors hover:bg-background" onClick={() => onSelect(r.key)}>
                {row}
              </button>
            ) : (
              <div key={r.key}>{row}</div>
            );
          })}
        </div>
      )}
    </div>
  );
}
