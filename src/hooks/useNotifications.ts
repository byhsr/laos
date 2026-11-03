import { create } from 'zustand';
import {
  clearNotifications, dismissNotification, listNotifications, markAllNotificationsRead,
  markNotificationRead, notificationUnreadCount, type Notification,
} from '../runtime';

// The notification inbox: the durable queue behind reminders, insights and
// finished work. `unread` drives the rail badge; `receive` is fed by the live
// `lumin-notification` event so a fresh alert appears without a reload.
type State = {
  items: Notification[];
  unread: number;
  load: () => Promise<void>;
  markRead: (id: number) => Promise<void>;
  dismiss: (id: number) => Promise<void>;
  markAllRead: () => Promise<void>;
  clear: () => Promise<void>;
  receive: (n: { id: number; kind: string; title: string; body: string; refKind: string; refId: string }) => void;
};

export const useNotifications = create<State>((set, get) => ({
  items: [],
  unread: 0,
  load: async () => {
    const [items, unread] = await Promise.all([listNotifications(false), notificationUnreadCount()]);
    set({ items, unread });
  },
  markRead: async (id) => { await markNotificationRead(id); await get().load(); },
  dismiss: async (id) => { await dismissNotification(id); await get().load(); },
  markAllRead: async () => { await markAllNotificationsRead(); await get().load(); },
  clear: async () => { await clearNotifications(); await get().load(); },
  receive: (n) => set((s) => ({
    items: [{
      id: n.id, kind: n.kind, title: n.title, body: n.body, refKind: n.refKind, refId: n.refId,
      createdAt: new Date().toISOString(), readAt: null, dismissedAt: null,
    }, ...s.items],
    unread: s.unread + 1,
  })),
}));
