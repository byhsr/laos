import { Home, LayoutGrid } from 'lucide-react';
import type { Agent, View } from '../types';
import { AgentAvatar } from './ui/AgentAvatar';
import { Tooltip } from './ui/Tooltip';
import { navLabel, useNavLabels } from '../hooks/useNavLabels';

// The sidebar is the agent chat list: click an agent to open its chat. The
// workspace sections live in the topbar; Home lives here.
export function Sidebar({ agents, view, selectedAgentId, homeActive, onOpen, onBrowse, onHome, collapsed }: {
  agents: Agent[]; view: View; selectedAgentId: string | null; homeActive: boolean;
  onOpen: (id: string) => void; onBrowse: () => void; onHome: () => void; collapsed: boolean;
}) {
  const labels = useNavLabels((s) => s.labels);
  // The lead agent always takes the first slot and cannot be removed.
  const lead = agents.find((a) => a.isManager) ?? null;
  const list = lead ? [lead, ...agents.filter((a) => !a.isManager)] : agents;
  const headerBtn = (active: boolean) =>
    `grid h-6 w-6 shrink-0 cursor-pointer place-items-center rounded-md transition-colors duration-150 ${active ? 'bg-line text-text' : 'text-muted hover:bg-line hover:text-text'}`;

  const row = (active: boolean) =>
    `flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl border px-2.5 py-2 text-left text-[13px] text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;
  const rowCollapsed = (active: boolean) =>
    `grid w-full cursor-pointer select-none place-items-center rounded-xl border py-2 text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;

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
          return (
            <Tooltip key={a.id} label={a.name} side="right">
              <button className={collapsed ? rowCollapsed(active) : row(active)} onClick={() => onOpen(a.id)}>
                <AgentAvatar agent={a} size={collapsed ? 26 : 22} />
                {!collapsed && <span className="min-w-0 truncate">{a.name}</span>}
              </button>
            </Tooltip>
          );
        })}
      </nav>
    </aside>
  );
}
