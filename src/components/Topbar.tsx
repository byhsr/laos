import { useEffect, useState } from 'react';
import { ChevronRight, Maximize, Minimize2, Minus, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Agent, Run, View } from '../types';
import { Vitals } from './Vitals';
import { navLabel, useNavLabels, type NavKey } from '../hooks/useNavLabels';
import { IconButton } from './ui/Button';

// The canvas chrome: no bar, no border — just the location trail floating on the
// left and the vitals + window controls floating on the right. Everything
// between them is free canvas, and this row is the window's drag handle.
type Crumb = { label: string; onClick?: () => void };

export function Topbar({ view, setView, agents, runs, onOpenGraph, onOpenHome, graphActive, selectedAgent, workflow, onExitWorkflow }: {
  view: View; setView: (v: View) => void;
  agents: Agent[]; runs: Run[]; onOpenGraph: () => void; onOpenHome: () => void; graphActive: boolean;
  selectedAgent: Agent | null; workflow: { id: string; name: string } | null; onExitWorkflow: () => void;
}) {
  const [maximized, setMaximized] = useState(false);
  const labels = useNavLabels((s) => s.labels);

  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    win.isMaximized().then(setMaximized).catch(() => {});
    win.onResized(() => win.isMaximized().then(setMaximized).catch(() => {})).then((fn) => { unlisten = fn; }).catch(() => {});
    return () => { unlisten?.(); };
  }, []);

  const minimize = () => { getCurrentWindow().minimize().catch(() => {}); };
  const toggleMax = () => { getCurrentWindow().toggleMaximize().catch(() => {}); };
  const close = () => { getCurrentWindow().close().catch(() => {}); };

  const winBtn = 'h-6 w-8 rounded';

  // Where you are, expressed as a trail: the section, then the entity open on
  // top of it. Parent segments navigate; the last one is just the location.
  const lead = agents.find((a) => a.isManager) ?? null;
  const section = (key: NavKey, v: View): Crumb => ({ label: navLabel(labels, key), onClick: view === v ? undefined : () => setView(v) });
  const crumbs: Crumb[] = (() => {
    switch (view) {
      case 'agent':
        return [
          { label: navLabel(labels, 'agents'), onClick: () => setView('agents') },
          { label: selectedAgent?.name ?? 'agent' },
        ];
      case 'workflows':
        return workflow
          ? [{ label: navLabel(labels, 'workflows'), onClick: onExitWorkflow }, { label: workflow.name }]
          : [section('workflows', 'workflows')];
      case 'home':
        return graphActive
          ? [{ label: navLabel(labels, 'home'), onClick: onOpenHome }, { label: navLabel(labels, 'graph') }]
          : [section('home', 'home')];
      case 'manager':
        return [{ label: lead?.name ?? 'agent' }];
      case 'agents': return [section('agents', 'agents')];
      case 'tasks': return [section('tasks', 'tasks')];
      case 'telegram': return [section('telegram', 'telegram')];
      case 'runs': return [section('runs', 'runs')];
      case 'workshop': return [section('workshop', 'workshop')];
      case 'settings': return [section('settings', 'settings')];
      default: return [];
    }
  })();

  return (
    <div className="app-drag relative z-50 flex h-9 flex-none items-center gap-2 px-2 select-none">
      {/* Left: the location trail — recedes until hovered */}
      <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden">
        {crumbs.map((c, i) => (
          <span key={`${c.label}-${i}`} className="flex min-w-0 items-center gap-1.5">
            {i > 0 && <ChevronRight size={11} className="shrink-0 text-muted" />}
            {c.onClick ? (
              <button
                type="button"
                onClick={c.onClick}
                className="app-no-drag focus-ring min-w-0 cursor-pointer truncate rounded border-0 bg-transparent p-0 font-mono text-[11px] lowercase text-muted transition-colors duration-150 hover:text-foreground"
              >
                {c.label}
              </button>
            ) : (
              <span className="min-w-0 truncate font-mono text-[11px] lowercase text-foreground">{c.label}</span>
            )}
          </span>
        ))}
      </div>

      {/* Right: vitals + window controls, floating over the canvas */}
      <div className="flex shrink-0 items-center gap-1.5">
        <div className="flex items-center rounded-lg border border-border bg-surface">
          <Vitals agents={agents} runs={runs} />
        </div>

        <div className="flex items-center gap-0.5 rounded-lg border border-border bg-surface p-1">
          <IconButton className={winBtn} label="minimize" onClick={minimize}><Minus className="h-3 w-3" strokeWidth={2.5} /></IconButton>
          <IconButton className={winBtn} label={maximized ? 'restore' : 'maximize'} onClick={toggleMax}>
            {maximized ? <Minimize2 className="h-3 w-3" /> : <Maximize className="h-3 w-3" />}
          </IconButton>
          <IconButton className={`${winBtn} hover:bg-danger hover:text-white`} label="close" onClick={close}><X className="h-3 w-3" /></IconButton>
        </div>
      </div>
    </div>
  );
}
