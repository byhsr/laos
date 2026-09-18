import { useEffect, useState } from 'react';
import { Maximize, Minimize2, Minus, PanelLeft, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function Topbar({ collapsed, onToggleSidebar }: { collapsed: boolean; onToggleSidebar: () => void }) {
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

  return (
    <div className="app-drag glass-strong relative z-50 flex h-8 flex-none items-stretch justify-between select-none">
      <div className="flex shrink-0 items-center">
        <button
          className="app-no-drag mx-2.5 grid shrink-0 cursor-pointer place-items-center self-center rounded-lg p-1.5 text-muted transition-colors duration-150 hover:bg-white/5 hover:text-text"
          onClick={onToggleSidebar}
          title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        >
          <PanelLeft className="h-3 w-3" />
        </button>
      </div>
      <div className="flex shrink-0 items-stretch">
        <button className="app-no-drag grid h-full w-10 shrink-0 cursor-pointer place-items-center border-0 bg-transparent text-muted transition-colors duration-150 hover:bg-white/5 hover:text-text" onClick={minimize} title="Minimize"><Minus className="h-3 w-3" strokeWidth={2.5} /></button>
        <button className="app-no-drag grid h-full w-10 shrink-0 cursor-pointer place-items-center border-0 bg-transparent text-muted transition-colors duration-150 hover:bg-white/5 hover:text-text" onClick={toggleMax} title={maximized ? 'Restore' : 'Maximize'}>{maximized ? <Minimize2 className="h-3 w-3" /> : <Maximize className="h-3 w-3" />}</button>
        <button className="app-no-drag grid h-full w-10 shrink-0 cursor-pointer place-items-center border-0 bg-transparent text-muted transition-colors duration-150 hover:bg-[#e11d48] hover:text-white" onClick={close} title="Close"><X className="h-3 w-3" /></button>
      </div>
    </div>
  );
}
