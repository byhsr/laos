import { useState } from 'react';
import { ShieldAlert } from 'lucide-react';
import { useConfirmStore } from '../../hooks/useConfirm';
import { Button } from './Button';
import { FIELD_LABEL_CLS, INPUT_CLS } from './Input';

const TOOL_LABELS: Record<string, string> = {
  create_agent: 'create agent', update_agent: 'update agent', delete_agent: 'delete agent',
  create_workflow: 'create workflow', update_workflow: 'update workflow', delete_workflow: 'delete workflow',
  run_workflow: 'run workflow', configure_integration: 'configure integration',
  create_task: 'run task', cancel_task: 'cancel task', delegate_task: 'delegate task',
  run_command: 'run command',
};

// Confirmation for a state-changing tool call. This is the app's confirmation
// boundary: the popup is the confirmation, so the model is never asked to seek
// permission in prose.
//
// It is a *floating panel*, not a full overlay, so it stays anchored where the
// user is looking: just above the composer, built from the shared popover shell.
export function ConfirmDialog() {
  const pending = useConfirmStore((s) => s.pending);
  const resolve = useConfirmStore((s) => s.resolve);
  const [args, setArgs] = useState<Record<string, unknown>>({});

  if (!pending) return null;
  // Re-sync editable args when a new request arrives.
  if (Object.keys(args).length === 0 && Object.keys(pending.args).length > 0) {
    setArgs({ ...pending.args });
  }

  const isSecret = (k: string) => /token|key|secret|password/i.test(k);
  const entries = Object.entries(args);
  const title = TOOL_LABELS[pending.tool] ?? pending.tool.replace(/_/g, ' ');

  return (
    <div className="animate-[ip-pop_100ms_var(--ease-panel)_both] rounded-xl border border-border bg-surface p-3.5 shadow-lg">
      <div className="mb-2.5 flex items-center gap-2">
        <ShieldAlert size={13} className="shrink-0 text-danger" />
        <b className="min-w-0 truncate font-mono text-xs lowercase text-foreground">confirm: {title}</b>
      </div>

      <div className="mb-3 grid gap-2">
        {entries.length === 0 && <p className="m-0 font-mono text-xs text-muted">no parameters</p>}
        {entries.map(([k, v]) => (
          <label key={k} className="block">
            <span className={FIELD_LABEL_CLS}>{k}</span>
            {Array.isArray(v) ? (
              <input
                value={v.join(', ')}
                onChange={(e) => setArgs((a) => ({ ...a, [k]: e.target.value.split(',').map((s) => s.trim()).filter(Boolean) }))}
                className={INPUT_CLS}
              />
            ) : (
              <input
                type={isSecret(k) ? 'password' : 'text'}
                value={typeof v === 'string' ? v : String(v ?? '')}
                onChange={(e) => setArgs((a) => ({ ...a, [k]: e.target.value }))}
                className={INPUT_CLS}
              />
            )}
          </label>
        ))}
      </div>

      <div className="flex justify-end gap-2">
        <Button onClick={() => resolve(false, args)}>reject</Button>
        <Button variant="accent" onClick={() => resolve(true, args)}>approve</Button>
      </div>
    </div>
  );
}
