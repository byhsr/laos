import { useEffect, useState } from 'react';
import { ArrowLeft, ArrowRight, Globe, Maximize, Minimize2, Minus, PanelLeft, Plus, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { navLabel, useNavLabels } from '../hooks/useNavLabels';
import { IconButton } from './ui/Button';

// The window chrome, kept to the minimum: one bar on the content plane holding
// the panel toggle, the history buttons, the graph / new-agent shortcuts and the
// window controls. No floating clusters — a single strip, and the row itself
// drags the window.
export function Topbar({ collapsed, onToggleSidebar, canBack, canForward, onBack, onForward, onOpenGraph, graphActive, onNewAgent }: {
  collapsed: boolean; onToggleSidebar: () => void;
  canBack: boolean; canForward: boolean; onBack: () => void; onForward: () => void;
  onOpenGraph: () => void; graphActive: boolean; onNewAgent: () => void;
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

  // The bar sits on the content plane, so a control hovers up to the chrome
  // plane rather than down to the background.
  const navBtn = ({ active = false, disabled = false }: { active?: boolean; disabled?: boolean } = {}) =>
    `app-no-drag grid h-6 w-6 shrink-0 place-items-center rounded border-0 transition-colors duration-150 focus-ring ${disabled ? 'cursor-not-allowed text-muted opacity-30' : `cursor-pointer ${active ? 'bg-surface text-foreground' : 'text-muted hover:bg-surface hover:text-foreground'}`}`;
  const winBtn = 'app-no-drag h-6 w-8 rounded text-muted hover:bg-surface hover:text-foreground';

  return (
    <div className="app-drag relative z-50 flex h-9 flex-none items-center gap-0.5 border-b border-border bg-background px-2 select-none">
      <IconButton className={navBtn()} label={collapsed ? 'expand chats' : 'collapse chats'} onClick={onToggleSidebar}>
        <PanelLeft className="h-3 w-3" />
      </IconButton>
      <span className="mx-0.5 h-3.5 w-px shrink-0 bg-border" />
      <IconButton className={navBtn({ disabled: !canBack })} label="back" onClick={onBack} disabled={!canBack}>
        <ArrowLeft className="h-3 w-3" />
      </IconButton>
      <IconButton className={navBtn({ disabled: !canForward })} label="forward" onClick={onForward} disabled={!canForward}>
        <ArrowRight className="h-3 w-3" />
      </IconButton>
      <span className="mx-0.5 h-3.5 w-px shrink-0 bg-border" />
      <IconButton className={navBtn({ active: graphActive })} label={navLabel(labels, 'graph')} onClick={onOpenGraph}>
        <Globe className="h-3 w-3" />
      </IconButton>
      <IconButton className={navBtn()} label="new agent" onClick={onNewAgent}>
        <Plus className="h-3 w-3" />
      </IconButton>

      <div className="ml-auto flex items-center gap-0.5">
        <IconButton className={winBtn} label="minimize" onClick={minimize}><Minus className="h-3 w-3" strokeWidth={2.5} /></IconButton>
        <IconButton className={winBtn} label={maximized ? 'restore' : 'maximize'} onClick={toggleMax}>
          {maximized ? <Minimize2 className="h-3 w-3" /> : <Maximize className="h-3 w-3" />}
        </IconButton>
        <IconButton className={`${winBtn} hover:bg-danger hover:text-white`} label="close" onClick={close}><X className="h-3 w-3" /></IconButton>
      </div>
    </div>
  );
}
