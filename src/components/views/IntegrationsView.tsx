import { Globe } from 'lucide-react';
import type { Integration } from '../../types';

export function IntegrationsView({ integrations }: {
  integrations: Integration[];
}) {
  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        <div><span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span><h1 style={{ margin: 0, fontSize: 24 }}>Integrations</h1></div>
      </header>
      <div className="grid max-w-[900px] gap-2.5">
        {integrations.map((i) => (
          <div key={i.id} className="flex items-center gap-[15px] rounded-[9px] border border-line bg-panel p-[18px]">
            <span className="grid h-10 w-10 place-items-center rounded-lg bg-panel2 text-[20px] text-[var(--purple)]"><Globe size={18} /></span>
            <div className="flex-1">
              <b style={{ fontSize: 13 }}>{i.name}</b>
              <span className="block text-[11px] text-muted">{i.kind} · {i.description}</span>
            </div>
            <span className="mr-[7px] text-[10px] text-muted"><i className={`mr-1.5 inline-block h-[7px] w-[7px] rounded-full ${i.configured ? 'bg-[var(--green)]' : 'bg-[#f79009]'}`} />{i.configured ? 'configured' : 'not configured'}</span>
            <span style={{ fontSize: 11 }}>{i.enabled ? 'on' : 'off'}</span>
          </div>
        ))}
      </div>
    </>
  );
}
