import { useState } from 'react';
import { Pin } from 'lucide-react';
import type { Agent, View } from '../types';
import { AgentAvatar } from './ui/AgentAvatar';
import { Tooltip } from './ui/Tooltip';
import { ContextMenuAt } from './ui/ContextMenu';
import { agentMenuItems } from './ui/agentMenu';
import { DeleteConfirm } from './ui/DeleteConfirm';

// The sidebar is purely the agent chat list: the lead agent always holds the
// first slot, pinned agents follow. Right-click a row for its actions. Every
// workspace section lives in the rail beside it.
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

  // Selection reads through neutral foreground + surface fill, never an outline
  // in a second colour.
  const row = (active: boolean) =>
    `focus-ring flex w-full cursor-pointer items-center gap-2.5 rounded-lg border-0 px-2.5 py-1.5 text-left font-mono text-[11px] transition-colors duration-150 ${active ? 'bg-background text-foreground' : 'bg-transparent text-muted hover:bg-background hover:text-foreground'}`;
  const rowCollapsed = (active: boolean) =>
    `focus-ring grid w-full cursor-pointer place-items-center rounded-lg border-0 py-2 transition-colors duration-150 ${active ? 'bg-background text-foreground' : 'bg-transparent text-muted hover:bg-background hover:text-foreground'}`;

  const closeDelete = () => setDeleteTarget(null);

  return (
    <aside className={`relative z-10 flex min-h-0 flex-col px-2 py-2.5 transition-[width] duration-200 ease-out ${collapsed ? 'w-[64px]' : 'w-[228px]'}`}>
      <nav className={`grid min-h-0 gap-0.5 overflow-x-hidden overflow-y-auto ${collapsed ? 'justify-items-center' : ''}`}>
        {list.length === 0 && !collapsed && <p className="m-0 px-1 font-mono text-[10px] leading-relaxed text-muted">no agents yet — use + in the rail.</p>}
        {list.map((a) => {
          const active = a.isManager ? view === 'manager' : (view === 'agent' && selectedAgentId === a.id);
          const button = (
            <button
              className={collapsed ? rowCollapsed(active) : row(active)}
              onClick={() => onOpen(a.id)}
              onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, agent: a }); }}
            >
              <AgentAvatar agent={a} size={collapsed ? 24 : 20} />
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

      {menu && (
        <ContextMenuAt
          items={agentMenuItems(menu.agent, { onOpen, onSettings, onTogglePin, onDelete: (a) => setDeleteTarget(a) })}
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
        />
      )}

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
