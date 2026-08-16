import { useState } from 'react';
import { Check, Key, Plug, RefreshCw } from 'lucide-react';
import { SiAirtable, SiGoogle, SiNotion, SiTelegram } from 'react-icons/si';
import type { Integration } from '../../types';
import { useIntegrationsStore } from '../../hooks/useIntegrations';
import { toast } from '../../hooks/useToast';
import { Drawer } from '../ui/Drawer';

const LOGOS: Record<string, React.ReactNode> = {
  notion: <SiNotion size={18} />,
  airtable: <SiAirtable size={18} />,
  sheets: <SiGoogle size={18} />,
  docs: <SiGoogle size={18} />,
  telegram: <SiTelegram size={18} />,
};

const OAUTH_PROVIDERS = ['sheets', 'docs', 'notion'];

export function IntegrationsView({ integrations }: { integrations: Integration[] }) {
  const saveConfig = useIntegrationsStore((s) => s.saveConfig);
  const connect = useIntegrationsStore((s) => s.connect);
  const test = useIntegrationsStore((s) => s.test);
  const loadIntegrations = useIntegrationsStore((s) => s.loadIntegrations);
  const [drawerId, setDrawerId] = useState<string | null>(null);
  const [tokenDraft, setTokenDraft] = useState<Record<string, string>>({});
  const [testing, setTesting] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const active = integrations.find((i) => i.id === drawerId) ?? null;

  const saveToken = async (id: string) => {
    const cfg: Record<string, unknown> = {};
    if (tokenDraft.token) cfg.token = tokenDraft.token;
    if (tokenDraft.apiKey) cfg.apiKey = tokenDraft.apiKey;
    if (tokenDraft.clientId) cfg.clientId = tokenDraft.clientId;
    if (tokenDraft.clientSecret) cfg.clientSecret = tokenDraft.clientSecret;
    setSaving(true);
    try {
      await saveConfig(id, cfg);
      toast(`${integrations.find((i) => i.id === id)?.name ?? id} configured`, 'success');
      setDrawerId(null);
      setTokenDraft({});
      // If this is an OAuth integration and creds are now saved, kick off Connect.
      if (OAUTH_PROVIDERS.includes(id) && cfg.clientId && cfg.clientSecret) {
        await onConnect(id);
      }
    } finally {
      setSaving(false);
    }
  };

  const onConnect = async (id: string) => {
    const def = integrations.find((i) => i.id === id);
    const hasClientId = def && typeof def.config.clientId === 'string' && (def.config.clientId as string).length > 0;
    if (!hasClientId) {
      // No OAuth client configured yet — open the config panel so the user can set it.
      setDrawerId(id);
      toast(`Set the OAuth client ID/secret for ${def?.name ?? id} first, then click Connect.`, 'info');
      return;
    }
    try {
      const url = await connect(id);
      toast('OAuth flow opened in your browser. Complete it there, then check back here.', 'info');
      void url;
    } catch (e) {
      const msg = typeof e === 'string' ? e : (e instanceof Error ? e.message : JSON.stringify(e));
      toast(msg || 'Connection failed', 'error');
    }
  };

  const onTest = async (id: string) => {
    setTesting(id);
    const ok = await test(id);
    setTesting(null);
    await loadIntegrations();
    toast(ok ? 'Connection OK' : 'Connection failed', ok ? 'success' : 'error');
  };

  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        <div><span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span><h1 style={{ margin: 0, fontSize: 24 }}>Integrations</h1></div>
      </header>
      <div className="grid max-w-[1100px] grid-cols-1 gap-2.5 md:grid-cols-2 xl:grid-cols-3">
        {integrations.map((i) => {
          const isOAuth = OAUTH_PROVIDERS.includes(i.id);
          return (
            <div key={i.id} className="rounded-[9px] border border-line bg-panel p-[18px]">
              <div className="flex items-center gap-[15px]">
                <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-panel2 text-text">{LOGOS[i.id] ?? <Key size={18} className="text-muted" />}</span>
                <div className="min-w-0 flex-1">
                  <b style={{ fontSize: 13 }}>{i.name}</b>
                  <span className="block text-[11px] text-muted">{i.provider} · {i.actions.length} actions</span>
                </div>
                <span className="mr-[7px] shrink-0 text-[10px] text-muted">
                  <i className={`mr-1.5 inline-block h-[7px] w-[7px] rounded-full ${i.connected ? 'bg-[var(--green)]' : 'bg-[#f79009]'}`} />
                  {i.connected ? 'Connected' : 'Not connected'}
                </span>
              </div>

              <div className="mt-3.5 flex flex-wrap items-center gap-2">
                {i.connected ? (
                  <>
                    <button className="secondary" onClick={() => onTest(i.id)} disabled={testing === i.id}><RefreshCw size={12} />{testing === i.id ? 'Testing…' : 'Test'}</button>
                    <button className="secondary" onClick={() => setDrawerId(i.id)}><Key size={12} />Config</button>
                  </>
                ) : isOAuth ? (
                  <button className="primary" onClick={() => onConnect(i.id)}><Plug size={13} />Connect</button>
                ) : (
                  <button className="primary" onClick={() => setDrawerId(i.id)}><Key size={13} />Configure</button>
                )}
              </div>

              {i.connected && i.actions.length > 0 && (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {i.actions.map((a) => (
                    <span key={a.name} title={a.description} className="rounded bg-panel2 px-2 py-1 font-mono text-[10px] text-muted">{a.name}</span>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {active && (
        <Drawer
          title={`${active.name} — config`}
          onClose={() => { setDrawerId(null); setTokenDraft({}); }}
          initialWidth={Math.round(window.innerWidth / 2)}
          resizable
          headerAction={
            <button className="primary" disabled={saving} onClick={() => saveToken(active.id)}>
              <Check size={13} />{saving ? 'Saving…' : 'Save config'}
            </button>
          }
        >
          <div className="w-full rounded-[10px] border border-line bg-panel p-[22px]">
            <p className="text-[12px] leading-[1.6] text-muted" style={{ margin: '0 0 14px' }}>
              Credentials are stored locally in the app's SQLite database — they never leave your machine.
            </p>

            <label className="mt-0 mb-1.5 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">TOKEN / API KEY</label>
            <input
              type="password"
              value={tokenDraft.token ?? tokenDraft.apiKey ?? ''}
              onChange={(e) => setTokenDraft((d) => ({ ...d, token: e.target.value, apiKey: e.target.value }))}
              placeholder={active.connected ? 'Leave blank to keep existing' : 'Paste token or API key'}
              className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid"
            />

            {OAUTH_PROVIDERS.includes(active.id) && (
              <>
                <label className="mt-4 mb-1.5 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">OAUTH CLIENT ID</label>
                <input value={tokenDraft.clientId ?? ''} onChange={(e) => setTokenDraft((d) => ({ ...d, clientId: e.target.value }))} placeholder="OAuth client ID" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid" />
                <label className="mt-4 mb-1.5 block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">OAUTH CLIENT SECRET</label>
                <input type="password" value={tokenDraft.clientSecret ?? ''} onChange={(e) => setTokenDraft((d) => ({ ...d, clientSecret: e.target.value }))} placeholder="OAuth client secret" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid" />
                <p className="mt-1.5 text-[11px] leading-[1.5] text-muted">Set client ID/secret, then click Connect to start the OAuth flow.</p>
              </>
            )}

            <div className="mt-4 flex items-center gap-2">
              {OAUTH_PROVIDERS.includes(active.id) && (
                <button className="secondary" onClick={() => onConnect(active.id)}><Plug size={12} />Connect</button>
              )}
            </div>
          </div>
        </Drawer>
      )}
    </>
  );
}
