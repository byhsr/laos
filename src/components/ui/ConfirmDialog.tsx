import { useState } from 'react';
import { Check, ShieldAlert, X } from 'lucide-react';
import { useConfirmStore } from '../../hooks/useConfirm';

const TOOL_LABELS: Record<string, string> = {
  create_agent: 'Create agent', update_agent: 'Update agent', delete_agent: 'Delete agent',
  create_workflow: 'Create workflow', update_workflow: 'Update workflow', delete_workflow: 'Delete workflow',
  run_workflow: 'Run workflow', configure_integration: 'Configure integration',
  create_task: 'Run task', cancel_task: 'Cancel task', delegate_task: 'Delegate task',
};

// Inline confirmation panel, rendered above the chat input (not a center modal).
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
    <div className="mb-2 rounded-[10px] border border-[#facc15]/40 bg-panel p-3.5 shadow-[0_8px_30px_#0008]">
      <div className="mb-2 flex items-center gap-2">
        <ShieldAlert size={15} className="text-[#facc15]" />
        <b className="text-[13px]">Laos wants to {title}</b>
      </div>
      <div className="mb-3 grid gap-2">
        {entries.length === 0 && <p className="text-[12px] text-muted">No parameters.</p>}
        {entries.map(([k, v]) => (
          <label key={k} className="block">
            <span className="mb-1 block font-mono text-[10px] uppercase tracking-[0.08em] text-muted">{k}</span>
            {Array.isArray(v) ? (
              <input
                value={Array.isArray(v) ? v.join(', ') : String(v ?? '')}
                onChange={(e) => setArgs((a) => ({ ...a, [k]: e.target.value.split(',').map((s) => s.trim()).filter(Boolean) }))}
                className="w-full rounded-md border border-line bg-panel2 px-3 py-1.5 text-[12.5px] text-text outline-none focus:border-mid"
              />
            ) : (
              <input
                type={isSecret(k) ? 'password' : 'text'}
                value={typeof v === 'string' ? v : String(v ?? '')}
                onChange={(e) => setArgs((a) => ({ ...a, [k]: e.target.value }))}
                className="w-full rounded-md border border-line bg-panel2 px-3 py-1.5 text-[12.5px] text-text outline-none focus:border-mid"
              />
            )}
          </label>
        ))}
      </div>
      <div className="flex justify-end gap-2">
        <button className="secondary" onClick={() => resolve(false, args)}><X size={13} />Reject</button>
        <button className="primary" onClick={() => resolve(true, args)}><Check size={13} />Approve</button>
      </div>
    </div>
  );
}
