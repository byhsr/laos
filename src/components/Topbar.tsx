import { useEffect, useState } from 'react';
import { Blocks, Globe, Home, ListChecks, Maximize, Minimize2, Minus, PanelLeft, Plus, Send, Settings, UserCog, Workflow, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Agent, Run, View } from '../types';
import { Vitals } from './Vitals';
import { navLabel, useNavLabels, type NavKey } from '../hooks/useNavLabels';

// Every workspace section lives in the topbar; the sidebar is the agent chat list.
const NAV: { key: NavKey; view: View; icon: React.ReactNode }[] = [
  { key: 'home', view: 'home', icon: <Home size={12} /> },
  { key: 'manager', view: 'manager', icon: <UserCog size={12} /> },
  { key: 'workflows', view: 'workflows', icon: <Workflow size={12} /> },
  { key: 'tasks', view: 'tasks', icon: <ListChecks size={12} /> },
  { key: 'workshop', view: 'workshop', icon: <Blocks size={12} /> },
  { key: 'telegram', view: 'telegram', icon: <Send size={12} /> },
  { key: 'settings', view: 'settings', icon: <Settings size={12} /> },
];

export function Topbar({ collapsed, onToggleSidebar, view, setView, onNewAgent, agents, runs, onOpenGraph, graphActive }: {
  collapsed: boolean; onToggleSidebar: () => void;
  view: View; setView: (v: View) => void; onNewAgent: () => void;
  agents: Agent[]; runs: Run[]; onOpenGraph: () => void; graphActive: boolean;
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

  const winBtn = 'app-no-drag grid h-5 w-7 shrink-0 cursor-pointer place-items-center rounded-md border-0 bg-transparent text-muted transition-colors duration-150 hover:bg-white/5 hover:text-text';
  const iconBtn = 'app-no-drag grid h-5 w-5 shrink-0 cursor-pointer place-items-center rounded-md text-muted transition-colors duration-150 hover:bg-white/5 hover:text-text';
  const navBtn = (active: boolean) =>
    `app-no-drag flex shrink-0 cursor-pointer items-center gap-1.5 rounded-lg px-2 py-1 text-[11px] transition-colors duration-150 ${active ? 'bg-white/10 text-text' : 'text-muted hover:bg-white/5 hover:text-text'}`;

  return (
    <div className="app-drag relative z-50 flex h-9 flex-none items-center justify-between gap-2 px-2 pt-1.5 select-none">
      {/* Left: shell toggle + every section + new agent */}
      <div className="glass flex min-w-0 items-center gap-0.5 rounded-lg p-1">
        <button className={iconBtn} onClick={onToggleSidebar} title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}>
          <PanelLeft className="h-3 w-3" />
        </button>
        <span className="mx-0.5 h-3.5 w-px shrink-0 bg-hairline" />
        {NAV.map((n) => (
          <button key={n.key} className={navBtn(view === n.view)} onClick={() => setView(n.view)} title={navLabel(labels, n.key)}>
            {n.icon}<span className="hidden truncate sm:inline">{navLabel(labels, n.key)}</span>
          </button>
        ))}
        <span className="mx-0.5 h-3.5 w-px shrink-0 bg-hairline" />
        <button className={iconBtn} onClick={onNewAgent} title="New agent">
          <Plus className="h-3 w-3" />
        </button>
      </div>

      {/* Right: graph, vitals, window controls — kept as separate floating clusters */}
      <div className="flex shrink-0 items-center gap-1.5">
        <div className="glass flex items-center gap-1 rounded-lg py-0.5 pr-1 pl-1">
          <button
            className={`app-no-drag grid h-5 w-5 shrink-0 cursor-pointer place-items-center rounded-md transition-colors duration-150 ${graphActive ? 'bg-white/10 text-text' : 'text-muted hover:bg-white/5 hover:text-text'}`}
            onClick={onOpenGraph}
            title="Graph"
          >
            <Globe className="h-3 w-3" />
          </button>
          <Vitals agents={agents} runs={runs} />
        </div>

        <div className="glass flex items-center gap-0.5 rounded-lg p-1">
          <button className={winBtn} onClick={minimize} title="Minimize"><Minus className="h-3 w-3" strokeWidth={2.5} /></button>
          <button className={winBtn} onClick={toggleMax} title={maximized ? 'Restore' : 'Maximize'}>{maximized ? <Minimize2 className="h-3 w-3" /> : <Maximize className="h-3 w-3" />}</button>
          <button className={`${winBtn} hover:bg-[#e11d48] hover:text-white`} onClick={close} title="Close"><X className="h-3 w-3" /></button>
        </div>
      </div>
    </div>
  );
}
