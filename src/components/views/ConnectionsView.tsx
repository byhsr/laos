import { useEffect, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button, IconButton } from '../ui/Button';
import { INPUT_CLS } from '../ui/Input';
import { Select } from '../ui/Select';
import { toast } from '../../hooks/useToast';
import { useConnectionsStore } from '../../hooks/useConnections';
import { IntegrationsView } from './IntegrationsView';
import type { Integration } from '../../types';

const RESOURCE_KINDS = ['sheet', 'doc', 'calendar', 'channel', 'repo', 'file', 'url', 'other'];

// The resource / connection / adapter layer, in one place: the adapters a
// connection can be made from, the connections the app holds (integrations and
// LLM endpoints), and the resources those connections expose. The integration
// config/OAuth flow is unchanged below, so existing setup keeps working.
export function ConnectionsView({ integrations }: { integrations: Integration[] }) {
  const adapters = useConnectionsStore((s) => s.adapters);
  const connections = useConnectionsStore((s) => s.connections);
  const resources = useConnectionsStore((s) => s.resources);
  const load = useConnectionsStore((s) => s.load);
  const addResource = useConnectionsStore((s) => s.addResource);
  const removeResource = useConnectionsStore((s) => s.removeResource);

  useEffect(() => { void load(); }, [load]);

  const [connId, setConnId] = useState('');
  const [kind, setKind] = useState('sheet');
  const [name, setName] = useState('');
  const [ref, setRef] = useState('');

  const add = async () => {
    if (!name.trim()) return;
    try {
      const url = /^https?:\/\//.test(ref.trim()) ? ref.trim() : '';
      await addResource({
        id: '', connectionId: connId, kind, name: name.trim(), externalId: ref.trim(), url,
        meta: {}, createdAt: '', updatedAt: '',
      });
      setName(''); setRef('');
      toast('resource saved', 'success');
    } catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };

  return (
    <div className="flex h-full flex-col">
      {/* The catalog: descriptors a connection can be created from. */}
      <Card className="mb-3 shrink-0">
        <div className="mb-2 font-mono text-[10px] tracking-wider text-muted uppercase">adapters</div>
        <div className="flex flex-wrap gap-1">
          {adapters.map((a) => (
            <span
              key={a.id}
              className="rounded border border-border px-1.5 py-0.5 font-mono text-[10px] text-muted"
              title={`${a.category} · ${a.provider} · ${a.auth}${a.actions.length ? ` · ${a.actions.length} actions` : ''}`}
            >
              {a.name}
            </span>
          ))}
          {adapters.length === 0 && <span className="font-mono text-[10px] text-muted">none yet.</span>}
        </div>
      </Card>

      {/* The configured instances. */}
      <Card className="mb-3 shrink-0">
        <div className="mb-2 font-mono text-[10px] tracking-wider text-muted uppercase">connections</div>
        <div className="grid gap-1.5">
          {connections.map((c) => (
            <div key={`${c.category}:${c.id}`} className="flex items-center gap-2.5">
              <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${c.connected ? 'bg-foreground' : 'bg-border'}`} title={c.connected ? 'connected' : 'not connected'} />
              <span className="font-mono text-[11px] text-foreground">{c.name}</span>
              <span className="font-mono text-[10px] text-muted">{c.category} · {c.id}</span>
            </div>
          ))}
          {connections.length === 0 && <span className="font-mono text-[10px] text-muted">nothing connected yet.</span>}
        </div>
      </Card>

      {/* The concrete objects a connection exposes. */}
      <Card className="mb-3 shrink-0">
        <div className="mb-2 font-mono text-[10px] tracking-wider text-muted uppercase">resources</div>
        <div className="flex flex-wrap items-end gap-2">
          <div className="w-[150px]"><Select value={connId} options={connections.map((c) => ({ value: c.id, label: c.name }))} onChange={setConnId} placeholder="connection ▸" /></div>
          <div className="w-[120px]"><Select value={kind} options={RESOURCE_KINDS.map((k) => ({ value: k, label: k }))} onChange={setKind} /></div>
          <div className="min-w-[150px] flex-1"><input value={name} onChange={(e) => setName(e.target.value)} placeholder="resource name" className={INPUT_CLS} /></div>
          <div className="min-w-[150px] flex-1"><input value={ref} onChange={(e) => setRef(e.target.value)} placeholder="id or url" className={INPUT_CLS} /></div>
          <Button variant="primary" icon={<Plus size={12} />} onClick={() => void add()}>add</Button>
        </div>
        <div className="mt-2 grid gap-1">
          {resources.map((r) => (
            <div key={r.id} className="group flex items-center gap-2.5 rounded-lg border border-border bg-surface px-2.5 py-1.5">
              <span className="shrink-0 rounded border border-border px-1.5 py-0.5 font-mono text-[9px] tracking-wider text-muted uppercase">{r.kind}</span>
              <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{r.name}</span>
              {r.externalId && <span className="max-w-[200px] truncate font-mono text-[10px] text-muted">{r.externalId}</span>}
              <IconButton label="delete" onClick={() => void removeResource(r.id)}><Trash2 size={11} /></IconButton>
            </div>
          ))}
          {resources.length === 0 && <span className="font-mono text-[10px] text-muted">no resources — add a sheet, calendar or doc that a connection exposes.</span>}
        </div>
      </Card>

      <div className="min-h-0 flex-1">
        <IntegrationsView integrations={integrations} />
      </div>
    </div>
  );
}
