import { useEffect, useState } from 'react';
import { AppWindow, ArrowLeft, ArrowRight, Bot, Boxes, Compass, FolderKanban, Globe, Maximize, Minimize2, Minus, PanelLeft, Plus, Sparkles, Workflow as WorkflowIcon, Wrench, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { navLabel, useNavLabels } from '../hooks/useNavLabels';
import { CREATE_KINDS, createLabel, useCreateDefault } from '../hooks/useCreateDefault';
import { ContextMenuAt, type MenuItem } from './ui/ContextMenu';
import { IconButton } from './ui/Button';
import type { CreateKind } from '../types';

// One icon per create action, shared by the "+" menu and its default rows.
const CREATE_ICON: Record<CreateKind, React.ReactNode> = {
  agent: <Bot size={13} />,
  workflow: <WorkflowIcon size={13} />,
  skill: <Sparkles size={13} />,
  script: <AppWindow size={13} />,
  project: <FolderKanban size={13} />,
  tool: <Wrench size={13} />,
  model: <Boxes size={13} />,
};

// The window chrome, kept to the minimum: one bar on the content plane holding
// the panel toggle, the history buttons, the graph / new-agent shortcuts and the
// window controls. No floating clusters — a single strip, and the row itself
// drags the window.
export function Topbar({ collapsed, onToggleSidebar, canBack, canForward, onBack, onForward, onOpenGraph, graphActive, onCreate, onToggleBrowser, browserActive }: {
  collapsed: boolean; onToggleSidebar: () => void;
  canBack: boolean; canForward: boolean; onBack: () => void; onForward: () => void;
  onOpenGraph: () => void; graphActive: boolean; onCreate: (kind: CreateKind) => void;
  onToggleBrowser: () => void; browserActive: boolean;
}) {
  const [maximized, setMaximized] = useState(false);
  const [newMenu, setNewMenu] = useState<{ x: number; y: number } | null>(null);
  const labels = useNavLabels((s) => s.labels);
  const createDefault = useCreateDefault((s) => s.kind);
  const setCreateDefault = useCreateDefault((s) => s.setKind);

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

  // Left-click makes the default kind; right-click offers them all and lets you
  // choose which one the left-click makes.
  const newItems: MenuItem[] = [
    ...CREATE_KINDS.map((c): MenuItem => ({
      key: c.kind, label: `new ${c.label}`, icon: CREATE_ICON[c.kind], onSelect: () => onCreate(c.kind),
    })),
    ...CREATE_KINDS.map((c, i): MenuItem => ({
      key: `default-${c.kind}`, label: `default · ${c.label}`, icon: CREATE_ICON[c.kind],
      dividerBefore: i === 0, active: createDefault === c.kind, onSelect: () => setCreateDefault(c.kind),
    })),
  ];

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
      <IconButton className={navBtn({ active: browserActive })} label="in-app browser" onClick={onToggleBrowser}>
        <Compass className="h-3 w-3" />
      </IconButton>
      <IconButton
        className={navBtn()}
        label={`new ${createLabel(createDefault)}`}
        onClick={() => onCreate(createDefault)}
        onContextMenu={(e) => { e.preventDefault(); setNewMenu({ x: e.clientX, y: e.clientY }); }}
      >
        <Plus className="h-3 w-3" />
      </IconButton>

      <div className="ml-auto flex items-center gap-0.5">
        <IconButton className={winBtn} label="minimize" onClick={minimize}><Minus className="h-3 w-3" strokeWidth={2.5} /></IconButton>
        <IconButton className={winBtn} label={maximized ? 'restore' : 'maximize'} onClick={toggleMax}>
          {maximized ? <Minimize2 className="h-3 w-3" /> : <Maximize className="h-3 w-3" />}
        </IconButton>
        <IconButton className={`${winBtn} hover:bg-danger hover:text-white`} label="close" onClick={close}><X className="h-3 w-3" /></IconButton>
      </div>

      {newMenu && (
        <ContextMenuAt items={newItems} x={newMenu.x} y={newMenu.y} onClose={() => setNewMenu(null)} />
      )}
    </div>
  );
}
