import { useEffect, useState } from 'react';
import { Globe, Maximize, Minimize2, Minus, PanelLeft, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Agent, Run } from '../types';
import { Vitals } from './Vitals';

export function Topbar({ collapsed, onToggleSidebar, agents, runs, onOpenGraph, graphActive }: {
  collapsed: boolean; onToggleSidebar: () => void; agents: Agent[]; runs: Run[];
  onOpenGraph: () => void; graphActive: boolean;
}) {
  const [maximized, setMaximized] = useState(false);

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

  // No bar background — just two floating clusters with open space between them,
  // so the workspace below reads as free surface rather than a framed panel.
  return (
    <div className="app-drag relative z-50 flex h-9 flex-none items-center justify-between px-2 pt-1.5 select-none">
      <div className="glass flex items-center gap-1.5 rounded-lg py-0.5 pr-1 pl-1">
        <button
          className="app-no-drag grid h-5 w-5 shrink-0 cursor-pointer place-items-center rounded-md text-muted transition-colors duration-150 hover:bg-white/5 hover:text-text"
          onClick={onToggleSidebar}
          title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        >
          <PanelLeft className="h-3 w-3" />
        </button>
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
  );
}
