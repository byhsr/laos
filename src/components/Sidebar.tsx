import { useState } from 'react';
import { Home, LayoutGrid, MessageSquare, Pin, PinOff, Settings, Trash2 } from 'lucide-react';
import type { Agent, View } from '../types';
import { AgentAvatar } from './ui/AgentAvatar';
import { Tooltip } from './ui/Tooltip';
import { ContextMenuAt, type MenuItem } from './ui/ContextMenu';
import { navLabel, useNavLabels } from '../hooks/useNavLabels';

// The sidebar is the agent chat list: the lead agent always holds the first slot,
// pinned agents follow. Right-click a row for its actions. The workspace sections
// live in the topbar.
export function Sidebar({ agents, view, selectedAgentId, homeActive, onOpen, onBrowse, onHome, onTogglePin, onSettings, onDelete, collapsed }: {
  agents: Agent[]; view: View; selectedAgentId: string | null; homeActive: boolean;
  onOpen: (id: string) => void; onBrowse: () => void; onHome: () => void;
  onTogglePin: (id: string, pinned: boolean) => void; onSettings: (id: string) => void; onDelete: (id: string) => Promise<void>;
  collapsed: boolean;
}) {
  const labels = useNavLabels((s) => s.labels);
  const [deleteTarget, setDeleteTarget] = useState<Agent | null>(null);
  const [typed, setTyped] = useState('');
  const [menu, setMenu] = useState<{ x: number; y: number; agent: Agent } | null>(null);

  const lead = agents.find((a) => a.isManager) ?? null;
  const rest = agents
    .filter((a) => !a.isManager)
    .sort((a, b) => Number(!!b.pinned) - Number(!!a.pinned) || a.name.localeCompare(b.name));
  const list = lead ? [lead, ...rest] : rest;

  const headerBtn = (active: boolean) =>
    `grid h-6 w-6 shrink-0 cursor-pointer place-items-center rounded-md transition-colors duration-150 ${active ? 'bg-line text-text' : 'text-muted hover:bg-line hover:text-text'}`;
  const row = (active: boolean) =>
    `flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl border px-2.5 py-2 text-left text-[13px] text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;
  const rowCollapsed = (active: boolean) =>
    `grid w-full cursor-pointer select-none place-items-center rounded-xl border py-2 text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;

  const closeDelete = () => { setDeleteTarget(null); setTyped(''); };

  // The lead agent can't be pinned or deleted.
  const menuItems = (a: Agent): MenuItem[] => [
    { key: 'open', label: 'Open chat', icon: <MessageSquare size={13} />, onSelect: () => onOpen(a.id) },
    { key: 'settings', label: 'Settings', icon: <Settings size={13} />, onSelect: () => onSettings(a.id) },
    ...(a.isManager ? [] : [
      {
        key: 'pin',
        label: a.pinned ? 'Unpin' : 'Pin to top',
        icon: a.pinned ? <PinOff size={13} /> : <Pin size={13} />,
        dividerBefore: true,
        onSelect: () => onTogglePin(a.id, !a.pinned),
      },
      {
        key: 'delete',
        label: 'Delete agent',
        icon: <Trash2 size={13} />,
        danger: true,
        onSelect: () => { setDeleteTarget(a); setTyped(''); },
      },
    ]),
  ];

  return (
    <aside className={`glass relative z-10 my-3 ml-3 flex flex-col rounded-3xl px-3 py-4 shadow-soft transition-[width] duration-200 ease-out ${collapsed ? 'w-[68px]' : 'w-[236px]'}`}>
      <div className={`mb-2 flex shrink-0 items-center gap-1 ${collapsed ? 'justify-center' : 'justify-between px-1'}`}>
        {!collapsed && <span className="font-mono text-[10.5px] tracking-[1px] text-muted">AGENTS</span>}
        <div className="flex items-center gap-0.5">
          <Tooltip label={navLabel(labels, 'home')} side="right">
            <button className={headerBtn(homeActive)} onClick={onHome}><Home size={13} /></button>
          </Tooltip>
          <Tooltip label="All agents" side="right">
            <button className={headerBtn(false)} onClick={onBrowse}><LayoutGrid size={13} /></button>
          </Tooltip>
        </div>
      </div>

      <nav className={`grid min-h-0 gap-1 overflow-y-auto ${collapsed ? 'justify-items-center' : ''}`}>
        {list.length === 0 && !collapsed && <p className="px-1 text-[11px] leading-1.6 text-muted">No agents yet — use + in the topbar.</p>}
        {list.map((a) => {
          const active = a.isManager ? view === 'manager' : (view === 'agent' && selectedAgentId === a.id);
          const button = (
            <button
              className={collapsed ? rowCollapsed(active) : row(active)}
              onClick={() => onOpen(a.id)}
              onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, agent: a }); }}
            >
              <AgentAvatar agent={a} size={collapsed ? 26 : 22} />
              {!collapsed && <span className="min-w-0 flex-1 truncate">{a.name}</span>}
              {!collapsed && a.pinned && <Pin size={11} className="shrink-0 text-muted" />}
            </button>
          );
          // The name is already on screen when expanded, so the tooltip would only
          // hang outside the panel — collapsed rows are the ones that need it.
          return collapsed
            ? <Tooltip key={a.id} label={a.name} side="right" className="flex w-full">{button}</Tooltip>
            : <div key={a.id} className="flex w-full">{button}</div>;
        })}
      </nav>

      {menu && <ContextMenuAt items={menuItems(menu.agent)} x={menu.x} y={menu.y} onClose={() => setMenu(null)} />}

      {/* Deleting needs the agent's name typed out, matching the agents browser. */}
      {deleteTarget && (
        <div className="fixed inset-0 z-[90] grid place-items-center bg-black/60 p-6" onClick={closeDelete}>
          <div className="glass-strong w-full max-w-sm rounded-2xl border border-hairline p-5 shadow-float" onClick={(e) => e.stopPropagation()}>
            <b className="text-[13px]">Delete "{deleteTarget.name}"?</b>
            <p className="mt-1.5 text-[11.5px] leading-1.6 text-muted">
              Type the agent's name to confirm. This removes the agent and its configuration.
            </p>
            <input
              autoFocus
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              placeholder={deleteTarget.name}
              className="mt-3 w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid"
            />
            <div className="mt-4 flex justify-end gap-2">
              <button className="secondary" onClick={closeDelete}>Cancel</button>
              <button
                className="cursor-pointer rounded-lg border px-3 py-1.5 text-[12px] font-semibold disabled:cursor-not-allowed disabled:opacity-40"
                style={{ background: '#e11d48', borderColor: '#e11d48', color: '#fff' }}
                disabled={typed.trim() !== deleteTarget.name}
                onClick={async () => { const id = deleteTarget.id; closeDelete(); await onDelete(id); }}
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}
