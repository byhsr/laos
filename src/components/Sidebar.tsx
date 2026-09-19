import { useState } from 'react';
import { MessageSquare, Pin, PinOff, Settings, Trash2 } from 'lucide-react';
import type { Agent, View } from '../types';
import { AgentAvatar } from './ui/AgentAvatar';
import { Tooltip } from './ui/Tooltip';
import { ContextMenuAt, type MenuItem } from './ui/ContextMenu';
import { DeleteConfirm } from './ui/DeleteConfirm';

// The sidebar is purely the agent chat list: the lead agent always holds the
// first slot, pinned agents follow. Right-click a row for its actions. Every
// workspace section lives in the topbar.
export function Sidebar({ agents, view, selectedAgentId, onOpen, onTogglePin, onSettings, onDelete, collapsed }: {
  agents: Agent[]; view: View; selectedAgentId: string | null;
  onOpen: (id: string) => void;
  onTogglePin: (id: string, pinned: boolean) => void; onSettings: (id: string) => void; onDelete: (id: string) => Promise<void>;
  collapsed: boolean;
}) {
  const [deleteTarget, setDeleteTarget] = useState<Agent | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; agent: Agent } | null>(null);

  const lead = agents.find((a) => a.isManager) ?? null;
  const rest = agents
    .filter((a) => !a.isManager)
    .sort((a, b) => Number(!!b.pinned) - Number(!!a.pinned) || a.name.localeCompare(b.name));
  const list = lead ? [lead, ...rest] : rest;

  const row = (active: boolean) =>
    `flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl border px-2.5 py-2 text-left text-[13px] text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;
  const rowCollapsed = (active: boolean) =>
    `grid w-full cursor-pointer select-none place-items-center rounded-xl border py-2 text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;

  const closeDelete = () => setDeleteTarget(null);

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
        onSelect: () => setDeleteTarget(a),
      },
    ]),
  ];

  return (
    <aside className={`glass relative z-10 my-3 ml-3 flex flex-col rounded-3xl px-3 py-4 shadow-soft transition-[width] duration-200 ease-out ${collapsed ? 'w-[68px]' : 'w-[236px]'}`}>
      <nav className={`grid min-h-0 gap-1 overflow-y-auto overflow-x-hidden ${collapsed ? 'justify-items-center' : ''}`}>
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

      {deleteTarget && (
        <DeleteConfirm
          name={deleteTarget.name}
          description="This permanently removes the agent and its configuration. Type the agent's name to confirm."
          onCancel={closeDelete}
          onConfirm={() => { const id = deleteTarget.id; closeDelete(); void onDelete(id); }}
        />
      )}
    </aside>
  );
}
