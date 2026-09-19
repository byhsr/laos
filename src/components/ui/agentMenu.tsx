import { MessageSquare, Pin, PinOff, Settings, Trash2 } from 'lucide-react';
import type { Agent } from '../../types';
import type { MenuItem } from './ContextMenu';

// The agent context menu, shared by the sidebar rows and the agents browser so
// the two can't drift apart. The lead agent can't be pinned or deleted.
export function agentMenuItems(a: Agent, handlers: {
  onOpen: (id: string) => void;
  onSettings: (id: string) => void;
  onTogglePin: (id: string, pinned: boolean) => void;
  onDelete: (a: Agent) => void;
}): MenuItem[] {
  return [
    { key: 'open', label: 'Open chat', icon: <MessageSquare size={13} />, onSelect: () => handlers.onOpen(a.id) },
    { key: 'settings', label: 'Settings', icon: <Settings size={13} />, onSelect: () => handlers.onSettings(a.id) },
    ...(a.isManager ? [] : [
      {
        key: 'pin',
        label: a.pinned ? 'Unpin' : 'Pin to top',
        icon: a.pinned ? <PinOff size={13} /> : <Pin size={13} />,
        dividerBefore: true,
        onSelect: () => handlers.onTogglePin(a.id, !a.pinned),
      },
      {
        key: 'delete',
        label: 'Delete agent',
        icon: <Trash2 size={13} />,
        danger: true,
        onSelect: () => handlers.onDelete(a),
      },
    ]),
  ];
}
