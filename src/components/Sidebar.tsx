import { Blocks, Bot, Home, ListChecks, Settings, UserCog, Workflow } from 'lucide-react';
import type { View } from '../types';

const NAV_ITEMS: { key: View; label: string; icon: React.ReactNode }[] = [
  { key: 'home', label: 'Home', icon: <Home size={15} /> },
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
    `flex w-full cursor-pointer select-none items-center gap-3 rounded-xl border px-3.5 py-2.5 text-left text-[13px] text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;
  const btnCollapsed = (active: boolean) =>
    `grid w-full cursor-pointer select-none place-items-center rounded-xl border px-0 py-2.5 text-muted transition-all duration-200 ease-out hover:bg-line hover:text-text ${active ? 'border-dotted border-line bg-panel2 text-text' : 'border-transparent bg-transparent'}`;

  return (
    <aside className={`glass relative z-10 my-3 ml-3 flex flex-col rounded-3xl px-4 py-5 shadow-soft transition-[width] duration-200 ease-out ${collapsed ? 'w-[68px]' : 'w-[236px]'}`}>
      <nav className={`grid gap-1.5 ${collapsed ? 'justify-items-center' : ''}`}>
        {NAV_ITEMS.map((it) => (
          <button key={it.key} className={collapsed ? btnCollapsed(view === it.key) : btn(view === it.key)} onClick={() => setView(it.key)} title={collapsed ? it.label : undefined}>
            {it.icon}{!collapsed && it.label}
          </button>
        ))}
      </nav>
      <div className="mt-auto pt-3">
        <button className={collapsed ? btnCollapsed(view === 'settings') : btn(view === 'settings')} onClick={() => setView('settings')} title={collapsed ? 'Settings' : undefined}>
          <Settings size={15} />{!collapsed && 'Settings'}
        </button>
      </div>
    </aside>
  );
}
