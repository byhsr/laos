import { Blocks, Bot, Home, ListChecks, Network, Settings, UserCog, Workflow } from 'lucide-react';
import type { View } from '../types';

const NAV_ITEMS: { key: View; label: string; icon: React.ReactNode }[] = [
  { key: 'home', label: 'Home', icon: <Home size={15} /> },
  { key: 'graph', label: 'Graph', icon: <Network size={15} /> },
  { key: 'manager', label: 'Laos', icon: <UserCog size={15} /> },
  { key: 'agents', label: 'Agents', icon: <Bot size={15} /> },
  { key: 'workflows', label: 'Workflows', icon: <Workflow size={15} /> },
  { key: 'tasks', label: 'Tasks', icon: <ListChecks size={15} /> },
  { key: 'workshop', label: 'Workshop', icon: <Blocks size={15} /> },
];

export function Sidebar({ view, setView, collapsed }: {
  view: View; setView: (v: View) => void; collapsed: boolean;
}) {
  const btn = (active: boolean) =>
    `flex w-full cursor-pointer items-center gap-[11px] rounded-[7px] border px-3 py-2 text-left text-[13px] text-muted transition-all duration-150 hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text hover:bg-panel2' : 'border-transparent bg-transparent hover:bg-line'}`;
  const btnCollapsed = (active: boolean) =>
    `grid w-full cursor-pointer place-items-center rounded-[7px] border px-0 py-2 text-muted transition-all duration-150 hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text hover:bg-panel2' : 'border-transparent bg-transparent hover:bg-line'}`;

  return (
    <aside className={`flex h-full flex-col border-r border-line bg-panel px-[13px] py-[23px] transition-[width] duration-[180ms] ${collapsed ? 'w-16' : 'w-[226px]'}`}>
      <nav className={`grid gap-[5px] ${collapsed ? 'justify-items-center' : ''}`}>
        {NAV_ITEMS.map((it) => (
          <button key={it.key} className={collapsed ? btnCollapsed(view === it.key) : btn(view === it.key)} onClick={() => setView(it.key)} title={collapsed ? it.label : undefined}>
            {it.icon}{!collapsed && it.label}
          </button>
        ))}
      </nav>
      <div className="mt-auto">
        <button className={collapsed ? btnCollapsed(view === 'settings') : btn(view === 'settings')} onClick={() => setView('settings')} title={collapsed ? 'Settings' : undefined}>
          <Settings size={15} />{!collapsed && 'Settings'}
        </button>
      </div>
    </aside>
  );
}
