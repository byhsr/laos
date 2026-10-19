import { Blocks, Bot, Brain, Home, Lightbulb, ListChecks, MoreHorizontal, Send, Settings, SquareTerminal, Terminal, Workflow } from 'lucide-react';
import type { View } from '../types';
import { navLabel, useNavLabels, type NavKey } from '../hooks/useNavLabels';
import { useSidebarTabs } from '../hooks/useSidebarTabs';
import { ContextMenu, type MenuItem } from './ui/ContextMenu';
import { IconButton } from './ui/Button';

// The action rail: every workspace section as a vertical icon strip, conjoined
// with the chat list in the one panel that owns the left section. Chrome is
// dense and icon-led; the whole rest of the window is free canvas.
const NAV: { key: NavKey; view: View; icon: React.ReactNode }[] = [
  { key: 'home', view: 'home', icon: <Home size={12} /> },
  { key: 'agents', view: 'agents', icon: <Bot size={12} /> },
  { key: 'memory', view: 'memory', icon: <Brain size={12} /> },
  { key: 'workflows', view: 'workflows', icon: <Workflow size={12} /> },
  { key: 'tasks', view: 'tasks', icon: <ListChecks size={12} /> },
  { key: 'workshop', view: 'workshop', icon: <Blocks size={12} /> },
  { key: 'runs', view: 'runs', icon: <Terminal size={12} /> },
  { key: 'telegram', view: 'telegram', icon: <Send size={12} /> },
  { key: 'terminal', view: 'terminal', icon: <SquareTerminal size={12} /> },
  { key: 'items', view: 'items', icon: <Lightbulb size={12} /> },
];

// The sections a user can hide. Graph lives in the topbar and Settings stays put,
// so neither is part of the rail. Exported for the Settings toggles.
export const RAIL_KEYS: NavKey[] = NAV.map((n) => n.key);

export function Rail({ view, setView }: { view: View; setView: (v: View) => void }) {
  const labels = useNavLabels((s) => s.labels);
  const hidden = useSidebarTabs((s) => s.hidden);
  const btn = (active = false) =>
    `grid h-6 w-6 shrink-0 cursor-pointer place-items-center rounded border transition-colors duration-150 focus-ring ${active ? 'border-border bg-background text-foreground' : 'border-transparent bg-transparent text-muted hover:bg-background hover:text-foreground'}`;

  // The active section always stays in the rail, so hiding the one you're on
  // can't strand you with nowhere to be.
  const shown = NAV.filter((n) => !hidden.includes(n.key) || n.view === view);

  // The overflow reaches every section, hidden or not, so nothing becomes
  // unreachable once it leaves the strip.
  const allSections: MenuItem[] = NAV.map((n) => ({
    key: n.key,
    label: navLabel(labels, n.key),
    icon: n.icon,
    active: n.view === view,
    onSelect: () => setView(n.view),
  }));

  return (
    <nav className="flex w-10 shrink-0 flex-col items-center gap-0.5 border-r border-border px-1 py-2.5">
      {shown.map((n) => (
        <IconButton
          key={n.key}
          className={btn(view === n.view)}
          label={navLabel(labels, n.key)}
          onClick={() => setView(n.view)}
        >
          {n.icon}
        </IconButton>
      ))}
      <div className="mt-auto flex flex-col items-center gap-0.5">
        {hidden.length > 0 && (
          <ContextMenu
            title="all sections"
            trigger={<MoreHorizontal className="h-3 w-3" />}
            triggerClassName={btn()}
            items={allSections}
          />
        )}
        <IconButton
          className={btn(view === 'settings')}
          label={navLabel(labels, 'settings')}
          onClick={() => setView('settings')}
        >
          <Settings className="h-3 w-3" />
        </IconButton>
      </div>
    </nav>
  );
}
