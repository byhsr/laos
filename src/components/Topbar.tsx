import { useEffect, useState } from 'react';
import { Blocks, Bot, Globe, Home, ListChecks, Maximize, Minimize2, Minus, PanelLeft, Plus, Send, Settings, Terminal, UserCog, Workflow, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Agent, Run, View } from '../types';
import { Vitals } from './Vitals';
import { navLabel, useNavLabels, type NavKey } from '../hooks/useNavLabels';
import { Tooltip } from './ui/Tooltip';

// Every section lives in the topbar; the sidebar is purely the agent chat list.
const NAV: { key: NavKey; view: View; icon: React.ReactNode }[] = [
  { key: 'home', view: 'home', icon: <Home size={12} /> },
  { key: 'manager', view: 'manager', icon: <UserCog size={12} /> },
  { key: 'agents', view: 'agents', icon: <Bot size={12} /> },
  { key: 'workflows', view: 'workflows', icon: <Workflow size={12} /> },
  { key: 'tasks', view: 'tasks', icon: <ListChecks size={12} /> },
  { key: 'workshop', view: 'workshop', icon: <Blocks size={12} /> },
  { key: 'runs', view: 'runs', icon: <Terminal size={12} /> },
  { key: 'telegram', view: 'telegram', icon: <Send size={12} /> },
  { key: 'settings', view: 'settings', icon: <Settings size={12} /> },
];

export function Topbar({ collapsed, onToggleSidebar, view, setView, onNewAgent, agents, runs, managerName, onOpenGraph, graphActive }: {
  collapsed: boolean; onToggleSidebar: () => void;
  view: View; setView: (v: View) => void; onNewAgent: () => void;
  agents: Agent[]; runs: Run[]; managerName: string; onOpenGraph: () => void; graphActive: boolean;
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
  const iconBtn = (active = false) =>
    `app-no-drag grid h-5 w-5 shrink-0 cursor-pointer place-items-center rounded-md transition-colors duration-150 ${active ? 'bg-white/10 text-text' : 'text-muted hover:bg-white/5 hover:text-text'}`;

  return (
    <div className="app-drag relative z-50 flex h-9 flex-none items-center justify-between gap-2 px-2 pt-1.5 select-none">
      {/* Left: shell toggle, graph, every section, new agent */}
      <div className="glass flex min-w-0 items-center gap-0.5 rounded-lg p-1">
        <Tooltip label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}>
          <button className={iconBtn()} onClick={onToggleSidebar}>
            <PanelLeft className="h-3 w-3" />
          </button>
        </Tooltip>
        <Tooltip label={navLabel(labels, 'graph')}>
          <button className={iconBtn(graphActive)} onClick={onOpenGraph}>
            <Globe className="h-3 w-3" />
          </button>
        </Tooltip>
        <span className="mx-0.5 h-3.5 w-px shrink-0 bg-hairline" />
        {NAV.map((n) => (
          <Tooltip key={n.key} label={n.view === 'manager' ? managerName : navLabel(labels, n.key)}>
            <button className={iconBtn(view === n.view)} onClick={() => setView(n.view)}>
              {n.icon}
            </button>
          </Tooltip>
        ))}
        <span className="mx-0.5 h-3.5 w-px shrink-0 bg-hairline" />
        <Tooltip label="New agent">
          <button className={iconBtn()} onClick={onNewAgent}>
            <Plus className="h-3 w-3" />
          </button>
        </Tooltip>
      </div>

      {/* Right: vitals + window controls — kept as separate floating clusters */}
      <div className="flex shrink-0 items-center gap-1.5">
        <div className="glass flex items-center gap-1 rounded-lg py-0.5 pr-1 pl-1">
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
