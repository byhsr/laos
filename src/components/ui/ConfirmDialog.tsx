import { useState } from 'react';
import { ShieldAlert } from 'lucide-react';
import { useConfirmStore, type PendingConfirm } from '../../hooks/useConfirm';
import { Button } from './Button';
import { Modal } from './Modal';
import { FIELD_LABEL_CLS, INPUT_CLS } from './Input';

const TOOL_LABELS: Record<string, string> = {
  create_agent: 'create agent', update_agent: 'update agent', delete_agent: 'delete agent',
  create_workflow: 'create workflow', update_workflow: 'update workflow', delete_workflow: 'delete workflow',
  run_workflow: 'run workflow', configure_integration: 'configure integration',
  create_task: 'run task', cancel_task: 'cancel task', delegate_task: 'delegate task',
  run_command: 'run command', run_script: 'run script',
  launch_app: 'launch app', close_window: 'close window', lock_screen: 'lock the screen',
  write_clipboard: 'write to the clipboard', open_path: 'open a path',
  browser_click: 'click in the browser', browser_type: 'type in the browser',
};

// The confirmation boundary for a state-changing tool call. The model is never
// asked to seek permission in prose — this modal is the ask, and it appears only
// when autonomy is off. A full overlay (the app's one confirmation grammar):
// Escape or ✕ rejects, which is the safe default.
export function ConfirmDialog() {
  const pending = useConfirmStore((s) => s.pending);
  const resolve = useConfirmStore((s) => s.resolve);
  if (!pending) return null;
  // Keyed on the request so the editable args reset cleanly for each new ask.
  return <ConfirmBody key={pending.requestId} pending={pending} resolve={resolve} />;
}

function ConfirmBody({ pending, resolve }: {
  pending: PendingConfirm;
  resolve: (approved: boolean, editedArgs: Record<string, unknown>) => void;
}) {
  const [args, setArgs] = useState<Record<string, unknown>>({ ...pending.args });

  const isSecret = (k: string) => /token|key|secret|password/i.test(k);
  const entries = Object.entries(args);
  const title = TOOL_LABELS[pending.tool] ?? pending.tool.replace(/_/g, ' ');

  return (
    <Modal title="approve tool" onClose={() => resolve(false, args)} width="440px">
      <div className="mb-3 flex items-start gap-2 rounded-lg border border-border bg-background px-3 py-2.5">
        <ShieldAlert size={14} className="mt-0.5 shrink-0 text-danger" />
        <div className="min-w-0">
          <b className="block font-mono text-xs lowercase text-foreground">{title}</b>
          <p className="mt-0.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">
            The assistant wants to do this once. Review the parameters, then approve or reject.
          </p>
        </div>
      </div>

      <div className="grid gap-2.5">
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

      <div className="mt-4 flex justify-end gap-2">
        <Button onClick={() => resolve(false, args)}>reject</Button>
        <Button variant="accent" onClick={() => resolve(true, args)}>approve</Button>
      </div>
    </Modal>
  );
}
