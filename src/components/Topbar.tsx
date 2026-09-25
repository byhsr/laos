import { useEffect, useState } from 'react';
import { Blocks, Bot, Globe, Home, ListChecks, Maximize, Minimize2, Minus, PanelLeft, Plus, Send, Settings, Terminal, Workflow, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Agent, Run, View } from '../types';
import { Vitals } from './Vitals';
import { navLabel, useNavLabels, type NavKey } from '../hooks/useNavLabels';
import { IconButton } from './ui/Button';

// Every section lives in the topbar; the sidebar is purely the agent chat list.
// Chrome sits on the surface plane as two edge-anchored clusters with open space
// between them, so the workspace reads as free space.
const NAV: { key: NavKey; view: View; icon: React.ReactNode }[] = [
  { key: 'home', view: 'home', icon: <Home size={12} /> },
  { key: 'agents', view: 'agents', icon: <Bot size={12} /> },
  { key: 'workflows', view: 'workflows', icon: <Workflow size={12} /> },
  { key: 'tasks', view: 'tasks', icon: <ListChecks size={12} /> },
  { key: 'workshop', view: 'workshop', icon: <Blocks size={12} /> },
  { key: 'runs', view: 'runs', icon: <Terminal size={12} /> },
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

  const cluster = 'flex items-center gap-0.5 rounded-lg border border-border bg-surface p-1';
  const iconBtn = (active = false) =>
    `app-no-drag grid h-6 w-6 shrink-0 cursor-pointer place-items-center rounded border-0 bg-transparent transition-colors duration-150 focus-ring ${active ? 'bg-background text-foreground' : 'text-muted hover:bg-background hover:text-foreground'}`;
  const winBtn = 'h-6 w-8 rounded';

  return (
    <div className="app-drag relative z-50 flex h-9 flex-none items-center justify-between gap-2 px-2 py-1 select-none">
      {/* Left: shell toggle, graph, every section, new agent */}
      <div className={`${cluster} min-w-0`}>
        <IconButton className={iconBtn()} label={collapsed ? 'expand sidebar' : 'collapse sidebar'} onClick={onToggleSidebar}>
          <PanelLeft className="h-3 w-3" />
        </IconButton>
        <IconButton className={iconBtn(graphActive)} label={navLabel(labels, 'graph')} onClick={onOpenGraph}>
          <Globe className="h-3 w-3" />
        </IconButton>
        <span className="mx-0.5 h-3.5 w-px shrink-0 bg-border" />
        {NAV.map((n) => (
          <IconButton
            key={n.key}
            className={iconBtn(view === n.view && !(n.view === 'home' && graphActive))}
            label={navLabel(labels, n.key)}
            onClick={() => setView(n.view)}
          >
            {n.icon}
          </IconButton>
        ))}
        <span className="mx-0.5 h-3.5 w-px shrink-0 bg-border" />
        <IconButton className={iconBtn()} label="new agent" onClick={onNewAgent}>
          <Plus className="h-3 w-3" />
        </IconButton>
      </div>

      {/* Right: vitals + window controls — kept as separate floating clusters */}
      <div className="flex shrink-0 items-center gap-1.5">
        <div className="flex items-center rounded-lg border border-border bg-surface">
          <Vitals agents={agents} runs={runs} />
        </div>

        <div className={cluster}>
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
