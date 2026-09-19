import { LayoutGrid } from 'lucide-react';
import type { Agent, View } from '../types';
import { AgentAvatar } from './ui/AgentAvatar';

// The sidebar is the agent chat list: click an agent to open its chat. Every
// workspace section lives in the topbar instead.
export function Sidebar({ agents, view, selectedAgentId, onOpen, onBrowse, collapsed }: {
  agents: Agent[]; view: View; selectedAgentId: string | null;
  onOpen: (id: string) => void; onBrowse: () => void; collapsed: boolean;
}) {
  const list = agents.filter((a) => !a.isManager);

  const row = (active: boolean) =>
    `flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl border px-2.5 py-2 text-left text-[13px] text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;
  const rowCollapsed = (active: boolean) =>
    `grid w-full cursor-pointer select-none place-items-center rounded-xl border py-2 text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;

  return (
    <aside className={`glass relative z-10 my-3 ml-3 flex flex-col rounded-3xl px-3 py-4 shadow-soft transition-[width] duration-200 ease-out ${collapsed ? 'w-[68px]' : 'w-[236px]'}`}>
      <div className={`mb-2 flex shrink-0 items-center ${collapsed ? 'justify-center' : 'justify-between px-1'}`}>
        {!collapsed && <span className="font-mono text-[10.5px] tracking-[1px] text-muted">AGENTS</span>}
        <button
          className="grid h-6 w-6 cursor-pointer place-items-center rounded-md text-muted transition-colors duration-150 hover:bg-line hover:text-text"
          onClick={onBrowse}
          title="All agents"
        >
          <LayoutGrid size={13} />
        </button>
      </div>

      <nav className={`grid min-h-0 gap-1 overflow-y-auto ${collapsed ? 'justify-items-center' : ''}`}>
        {list.length === 0 && !collapsed && <p className="px-1 text-[11px] leading-1.6 text-muted">No agents yet — use + in the topbar.</p>}
        {list.map((a) => {
          const active = view === 'agent' && selectedAgentId === a.id;
          return (
            <button key={a.id} className={collapsed ? rowCollapsed(active) : row(active)} onClick={() => onOpen(a.id)} title={collapsed ? a.name : undefined}>
              <AgentAvatar agent={a} size={collapsed ? 26 : 22} />
              {!collapsed && <span className="min-w-0 truncate">{a.name}</span>}
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
