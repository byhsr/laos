import { useState } from 'react';
import { Bot, Plus, Trash2 } from 'lucide-react';
import type { Agent } from '../../types';
import { AgentAvatar } from '../ui/AgentAvatar';
import { DeleteConfirm } from '../ui/DeleteConfirm';
import { ContextMenuAt } from '../ui/ContextMenu';
import { agentMenuItems } from '../ui/agentMenu';
import { Button, IconButton } from '../ui/Button';

export function AgentsView({ agents, onOpen, onCreate, onDelete, onSettings, onTogglePin }: {
  agents: Agent[]; onOpen: (id: string) => void; onCreate: () => void; onDelete: (id: string) => void;
  onSettings: (id: string) => void; onTogglePin: (id: string, pinned: boolean) => void;
}) {
  const visible = agents.filter((a) => !a.isManager);
  const [confirmTarget, setConfirmTarget] = useState<Agent | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; agent: Agent } | null>(null);

  const openConfirm = (a: Agent) => setConfirmTarget(a);
  const closeConfirm = () => setConfirmTarget(null);

  return (
    <>
      {/* No view title — the header row carries only the action. */}
      <div className="mb-4 flex justify-end">
        <Button variant="primary" icon={<Plus size={13} />} onClick={onCreate}>new agent</Button>
      </div>

      {visible.length === 0 ? (
        <div className="flex min-h-[300px] flex-col items-center justify-center rounded-xl border border-dashed border-border text-center text-muted">
          <Bot size={26} className="mb-3 opacity-60" />
          <h2 className="m-0 font-mono text-xs lowercase text-foreground">no agents yet</h2>
          <p className="mt-2 mb-4 max-w-[360px] text-[12px] leading-relaxed">Create your first agent — give it a name, a model, and an objective, then start chatting.</p>
          <Button variant="primary" icon={<Plus size={13} />} onClick={onCreate}>new agent</Button>
        </div>
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
          {visible.map((a) => (
            <div
              key={a.id}
              className="group relative cursor-pointer overflow-hidden rounded-xl border border-border bg-surface text-left transition-colors duration-150 hover:bg-background"
              onClick={() => onOpen(a.id)}
              onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, agent: a }); }}
              onMouseEnter={() => setHoveredId(a.id)}
              onMouseLeave={() => setHoveredId((h) => (h === a.id ? null : h))}
            >
              <div className="absolute top-2 right-2 z-[1] opacity-0 transition-opacity duration-150 group-hover:opacity-100">
                <IconButton
                  label="delete agent"
                  className="h-7 w-7 bg-surface hover:bg-danger hover:text-white"
                  onClick={(e) => { e.stopPropagation(); openConfirm(a); }}
                >
                  <Trash2 size={12} />
                </IconButton>
              </div>
              <AgentAvatar agent={a} playing={hoveredId === a.id} fluid />
              <div className="px-3.5 py-3">
                <b className="block truncate font-mono text-[11px] text-foreground">{a.name}</b>
              </div>
            </div>
          ))}
        </div>
      )}

      {menu && (
        <ContextMenuAt
          items={agentMenuItems(menu.agent, { onOpen, onSettings, onTogglePin, onDelete: openConfirm })}
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
        />
      )}

      {confirmTarget && (
        <DeleteConfirm
          name={confirmTarget.name}
          description="This permanently removes the agent and its configuration. Type the agent's name to confirm."
          onCancel={closeConfirm}
          onConfirm={() => { onDelete(confirmTarget.id); closeConfirm(); }}
        />
      )}
    </>
  );
}
