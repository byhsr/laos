import { useState } from 'react';
import { Check, Globe, Key, Plug, RefreshCw, X } from 'lucide-react';
import type { Integration } from '../../types';
import { useIntegrationsStore } from '../../hooks/useIntegrations';
import { toast } from '../../hooks/useToast';

const OAUTH_PROVIDERS = ['sheets', 'docs', 'notion'];

export function IntegrationsView({ integrations }: { integrations: Integration[] }) {
  const saveConfig = useIntegrationsStore((s) => s.saveConfig);
  const connect = useIntegrationsStore((s) => s.connect);
  const test = useIntegrationsStore((s) => s.test);
  const loadIntegrations = useIntegrationsStore((s) => s.loadIntegrations);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [tokenDraft, setTokenDraft] = useState<Record<string, string>>({});
  const [testing, setTesting] = useState<string | null>(null);

  const saveToken = async (id: string) => {
    const cfg: Record<string, unknown> = {};
    if (tokenDraft.token) cfg.token = tokenDraft.token;
    if (tokenDraft.apiKey) cfg.apiKey = tokenDraft.apiKey;
    if (tokenDraft.clientId) cfg.clientId = tokenDraft.clientId;
    if (tokenDraft.clientSecret) cfg.clientSecret = tokenDraft.clientSecret;
    await saveConfig(id, cfg);
    toast(`${integrations.find((i) => i.id === id)?.name ?? id} configured`, 'success');
    setExpanded(null);
    setTokenDraft({});
    // If this is an OAuth integration and creds are now saved, kick off Connect.
    if (OAUTH_PROVIDERS.includes(id) && cfg.clientId && cfg.clientSecret) {
      await onConnect(id);
    }
  };

  const onConnect = async (id: string) => {
    const def = integrations.find((i) => i.id === id);
    const hasClientId = def && typeof def.config.clientId === 'string' && (def.config.clientId as string).length > 0;
    if (!hasClientId) {
      // No OAuth client configured yet — open the config panel so the user can set it.
      setExpanded(id);
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
      <div className="grid max-w-[900px] gap-2.5">
        {integrations.map((i) => {
          const isOpen = expanded === i.id;
          const isOAuth = OAUTH_PROVIDERS.includes(i.id);
          return (
            <div key={i.id} className="rounded-[9px] border border-line bg-panel p-[18px]">
              <div className="flex items-center gap-[15px]">
                <span className="grid h-10 w-10 place-items-center rounded-lg bg-panel2 text-[20px] text-muted"><Globe size={18} /></span>
                <div className="flex-1">
                  <b style={{ fontSize: 13 }}>{i.name}</b>
                  <span className="block text-[11px] text-muted">{i.provider} · {i.actions.length} actions</span>
                </div>
                <span className="mr-[7px] text-[10px] text-muted">
                  <i className={`mr-1.5 inline-block h-[7px] w-[7px] rounded-full ${i.connected ? 'bg-[var(--green)]' : 'bg-[#f79009]'}`} />
                  {i.connected ? 'Connected' : 'Not connected'}
                </span>
                <div className="flex items-center gap-2">
                  {i.connected ? (
                    <>
                      <button className="secondary" onClick={() => onTest(i.id)} disabled={testing === i.id}><RefreshCw size={12} />{testing === i.id ? 'Testing…' : 'Test'}</button>
                      <button className="secondary" onClick={() => setExpanded(isOpen ? null : i.id)}>{isOpen ? <X size={12} /> : <Key size={12} />}{isOpen ? 'Close' : 'Config'}</button>
                    </>
                  ) : isOAuth ? (
                    <button className="primary" onClick={() => onConnect(i.id)}><Plug size={13} />Connect</button>
                  ) : (
                    <button className="primary" onClick={() => setExpanded(isOpen ? null : i.id)}><Key size={13} />Configure</button>
                  )}
                </div>
              </div>

              {isOpen && (
                <div className="mt-4 border-t border-line pt-4">
                  <div className="grid gap-2.5">
                    <label className="block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">TOKEN / API KEY</label>
                    <input
                      type="password"
                      value={tokenDraft.token ?? tokenDraft.apiKey ?? ''}
                      onChange={(e) => setTokenDraft((d) => ({ ...d, token: e.target.value, apiKey: e.target.value }))}
                      placeholder={i.connected ? 'Leave blank to keep existing' : 'Paste token or API key'}
                      className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid"
                    />
                    {isOAuth && (
                      <>
                        <label className="block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">OAUTH CLIENT ID</label>
                        <input value={tokenDraft.clientId ?? ''} onChange={(e) => setTokenDraft((d) => ({ ...d, clientId: e.target.value }))} placeholder="OAuth client ID" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid" />
                        <label className="block text-[10px] font-semibold tracking-[0.08em] text-muted uppercase">OAUTH CLIENT SECRET</label>
                        <input type="password" value={tokenDraft.clientSecret ?? ''} onChange={(e) => setTokenDraft((d) => ({ ...d, clientSecret: e.target.value }))} placeholder="OAuth client secret" className="w-full rounded-md border border-line bg-panel2 px-3 py-2 text-[12.5px] text-text outline-none focus:border-mid" />
                        <p className="text-[11px] leading-1.5 text-muted">Set client ID/secret, then click Connect to start the OAuth flow.</p>
                      </>
                    )}
                    <div className="flex items-center gap-2">
                      <button className="primary" onClick={() => saveToken(i.id)}><Check size={13} />Save config</button>
                      {isOAuth && <button className="secondary" onClick={() => onConnect(i.id)}><Plug size={12} />Connect</button>}
                    </div>
                  </div>
                </div>
              )}

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
    </>
  );
}
