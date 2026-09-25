import { useEffect, useState } from 'react';
import { Maximize, Minimize2, Minus, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { IconButton } from './ui/Button';

// The window chrome: no bar, no border, no readouts — just the window controls
// floating over the canvas. This row is the window's drag handle.
export function Topbar() {
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

  const winBtn = 'h-6 w-8 rounded';

  return (
    <div className="app-drag relative z-50 flex h-9 flex-none items-center justify-end gap-2 px-2 select-none">
      <div className="flex shrink-0 items-center gap-0.5 rounded-lg border border-border bg-surface p-1">
        <IconButton className={winBtn} label="minimize" onClick={minimize}><Minus className="h-3 w-3" strokeWidth={2.5} /></IconButton>
        <IconButton className={winBtn} label={maximized ? 'restore' : 'maximize'} onClick={toggleMax}>
          {maximized ? <Minimize2 className="h-3 w-3" /> : <Maximize className="h-3 w-3" />}
        </IconButton>
        <IconButton className={`${winBtn} hover:bg-danger hover:text-white`} label="close" onClick={close}><X className="h-3 w-3" /></IconButton>
      </div>
    </div>
  );
}
