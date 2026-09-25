import { useState } from 'react';
import { Check, Globe, Key, Loader2, Plug, RefreshCw, Rocket, X } from 'lucide-react';
import { SiAirtable, SiGoogle, SiNotion, SiTelegram } from 'react-icons/si';
import type { Integration } from '../../types';
import { useIntegrationsStore } from '../../hooks/useIntegrations';
import { toast } from '../../hooks/useToast';
import { Modal } from '../ui/Modal';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS } from '../ui/Input';
import { McpServers } from './McpServers';
import { telegramRegisterCustomUrl, telegramRegisterWebhook, telegramStartTunnel, telegramStopTunnel, telegramTunnelStatus } from '../../runtime';

// Third-party marks are rendered as themselves, in the app's foreground — the
// icons are currentColor, so no invented brand colours are layered on top.
const LOGOS: Record<string, React.ReactNode> = {
  notion: <SiNotion size={18} />,
  airtable: <SiAirtable size={18} />,
  sheets: <SiGoogle size={18} />,
  docs: <SiGoogle size={18} />,
  telegram: <SiTelegram size={18} />,
};

const OAUTH_PROVIDERS = ['sheets', 'docs', 'notion'];

// Backend progress lines arrive with a leading status glyph; the UI owns the
// icon, so strip any glyph the backend embedded in the text.
const cleanStep = (s: string) => s.replace(/^[✕✓▸·…\-]\s*/, '');
const stepFailed = (s: string) => s.startsWith('✕');

export function IntegrationsView({ integrations }: { integrations: Integration[] }) {
  const saveConfig = useIntegrationsStore((s) => s.saveConfig);
  const connect = useIntegrationsStore((s) => s.connect);
  const test = useIntegrationsStore((s) => s.test);
  const loadIntegrations = useIntegrationsStore((s) => s.loadIntegrations);
  const [drawerId, setDrawerId] = useState<string | null>(null);
  const [tokenDraft, setTokenDraft] = useState<Record<string, string>>({});
  const [testing, setTesting] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [tunnelBusy, setTunnelBusy] = useState(false);
  const [tunnelUrl, setTunnelUrl] = useState<string | null>(null);
  const [webhookRegistered, setWebhookRegistered] = useState(false);
  const [tunnelSteps, setTunnelSteps] = useState<string[]>([]);
  const [customUrl, setCustomUrl] = useState('');
  const [customBusy, setCustomBusy] = useState(false);

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
      toast(`set the OAuth client ID/secret for ${def?.name ?? id} first, then click connect`, 'info');
      return;
    }
    try {
      const url = await connect(id);
      toast('OAuth flow opened in your browser. Complete it there, then check back here.', 'info');
      void url;
    } catch (e) {
      const msg = typeof e === 'string' ? e : (e instanceof Error ? e.message : JSON.stringify(e));
      toast(msg || 'connection failed', 'error');
    }
  };

  const onTest = async (id: string) => {
    setTesting(id);
    const ok = await test(id);
    setTesting(null);
    await loadIntegrations();
    toast(ok ? 'connection ok' : 'connection failed', ok ? 'success' : 'error');
  };

  const refreshTunnelStatus = async () => {
    const st = await telegramTunnelStatus();
    setTunnelUrl(st.tunnelUrl);
    setWebhookRegistered(st.webhookRegistered);
    // If the tunnel is up but the webhook isn't registered, try again — the
    // token may have been saved since the last attempt.
    if (st.tunnelUrl && !st.webhookRegistered) {
      await onRegisterWebhook();
    }
  };

  const onRegisterWebhook = async () => {
    setTunnelBusy(true);
    setTunnelSteps([]);
    const addStep = (s: string) => setTunnelSteps((prev) => [...prev, s]);
    try {
      const msg = await telegramRegisterWebhook(addStep);
      setWebhookRegistered(true);
      addStep(msg);
      toast('webhook registered', 'success');
    } catch (e) {
      const msg = typeof e === 'string' ? e : 'Webhook registration failed';
      addStep(`✕ ${msg}`);
      toast(msg, 'error');
    } finally {
      setTunnelBusy(false);
    }
  };

  const onExposeTelegram = async () => {
    setTunnelBusy(true);
    setTunnelSteps([]);
    const addStep = (s: string) => setTunnelSteps((prev) => [...prev, s]);
    try {
      // Start the tunnel (spawns cloudflared), then register the webhook.
      const url = await telegramStartTunnel(addStep);
      setTunnelUrl(url);
      const msg = await telegramRegisterWebhook(addStep);
      setWebhookRegistered(true);
      addStep(msg);
      toast('telegram connected', 'success');
    } catch (e) {
      const msg = typeof e === 'string' ? e : 'Tunnel failed';
      addStep(`✕ ${msg}`);
      toast(msg, 'error');
    } finally {
      setTunnelBusy(false);
    }
  };

  const onStopTunnel = async () => {
    setTunnelBusy(true);
    try {
      await telegramStopTunnel();
      setTunnelUrl(null);
      setWebhookRegistered(false);
      toast('tunnel stopped', 'success');
    } finally {
      setTunnelBusy(false);
    }
  };

  const onRegisterCustomUrl = async () => {
    setCustomBusy(true);
    setTunnelSteps([]);
    const addStep = (s: string) => setTunnelSteps((prev) => [...prev, s]);
    try {
      const msg = await telegramRegisterCustomUrl(customUrl, addStep);
      setWebhookRegistered(true);
      setTunnelUrl(customUrl.trim().replace(/\/$/, ''));
      addStep(msg);
      toast('webhook registered', 'success');
    } catch (e) {
      const msg = typeof e === 'string' ? e : 'Registration failed';
      addStep(`✕ ${msg}`);
      toast(msg, 'error');
    } finally {
      setCustomBusy(false);
    }
  };

  const label = `${FIELD_LABEL_CLS} mt-4`;

  return (
    <>
      <div className="grid grid-cols-1 gap-2.5 md:grid-cols-2 xl:grid-cols-3">
        {integrations.map((i) => {
          const isOAuth = OAUTH_PROVIDERS.includes(i.id);
          return (
            <Card key={i.id} className="flex flex-col gap-3 hover:bg-surface">
              <div className="flex items-start justify-between gap-4">
                <div className="flex min-w-0 items-start gap-3.5">
                  <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-background text-foreground">{LOGOS[i.id] ?? <Key size={16} className="text-muted" />}</span>
                  <div className="min-w-0">
                    <b className="block truncate font-mono text-[11px] text-foreground">{i.name}</b>
                    <span className="mt-0.5 block truncate font-mono text-[10px] text-muted">{i.provider} · {i.actions.length} actions</span>
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  {i.connected ? (
                    <>
                      <IconButton label="test connection" onClick={() => onTest(i.id)} disabled={testing === i.id}>
                        <RefreshCw size={12} className={testing === i.id ? 'animate-spin' : ''} />
                      </IconButton>
                      <IconButton label="configure" onClick={() => setDrawerId(i.id)}><Key size={12} /></IconButton>
                    </>
                  ) : isOAuth ? (
                    <IconButton label="connect" className="border-foreground/30 text-foreground" onClick={() => onConnect(i.id)}><Plug size={12} /></IconButton>
                  ) : (
                    <IconButton label="configure" className="border-foreground/30 text-foreground" onClick={() => setDrawerId(i.id)}><Key size={12} /></IconButton>
                  )}
                </div>
              </div>

              {i.connected && i.actions.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {i.actions.map((a) => (
                    <span key={a.name} className="rounded bg-background px-2 py-0.5 font-mono text-[10px] text-muted">{a.name}</span>
                  ))}
                </div>
              )}

              <span className="flex items-center gap-1.5 font-mono text-[10px] text-muted">
                <i className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${i.connected ? 'bg-foreground' : 'bg-muted'}`} />
                {i.connected ? 'connected' : 'not connected'}
              </span>
            </Card>
          );
        })}
      </div>

      <McpServers />

      {active && (
        <Modal
          title={`${active.name} — config`}
          onClose={() => { setDrawerId(null); setTokenDraft({}); }}
          headerAction={
            <Button variant="primary" disabled={saving} onClick={() => saveToken(active.id)}>
              {saving ? 'saving…' : 'save config'}
            </Button>
          }
        >
          <p className="mt-0 mb-3.5 font-mono text-[10px] leading-relaxed text-muted">
            Credentials are stored locally in the app's SQLite database — they never leave your machine.
          </p>

          <label className={FIELD_LABEL_CLS}>token / api key</label>
          <input
            type="password"
            value={tokenDraft.token ?? tokenDraft.apiKey ?? ''}
            onChange={(e) => setTokenDraft((d) => ({ ...d, token: e.target.value, apiKey: e.target.value }))}
            placeholder={active.connected ? 'Leave blank to keep existing' : 'Paste token or API key'}
            className={INPUT_CLS}
          />

          {OAUTH_PROVIDERS.includes(active.id) && (
            <>
              <label className={label}>oauth client id</label>
              <input value={tokenDraft.clientId ?? ''} onChange={(e) => setTokenDraft((d) => ({ ...d, clientId: e.target.value }))} placeholder="OAuth client ID" className={INPUT_CLS} />
              <label className={label}>oauth client secret</label>
              <input type="password" value={tokenDraft.clientSecret ?? ''} onChange={(e) => setTokenDraft((d) => ({ ...d, clientSecret: e.target.value }))} placeholder="OAuth client secret" className={INPUT_CLS} />
              <p className="mt-1.5 mb-0 font-mono text-[10px] leading-relaxed text-muted">Set client ID/secret, then click connect to start the OAuth flow.</p>
              <div className="mt-3.5">
                <Button icon={<Plug size={12} />} onClick={() => onConnect(active.id)}>connect</Button>
              </div>
            </>
          )}

          {active.id === 'telegram' && (
            <>
              <div className="mt-5 rounded-xl border border-border bg-background p-4">
                <div className="mb-1.5 flex items-center justify-between gap-3">
                  <span className={GROUP_LABEL_CLS}>remote access</span>
                  <IconButton label="refresh status" className="h-6 w-6" onClick={refreshTunnelStatus}><RefreshCw size={12} /></IconButton>
                </div>
                <p className="mb-3 text-[12px] leading-[1.65] text-muted">
                  Expose this local app to Telegram with a Cloudflare tunnel. One click starts the tunnel and registers the webhook — no domain needed, Cloudflare gives you a free <code className="font-mono">trycloudflare.com</code> URL.
                </p>

                {tunnelUrl ? (
                  <>
                    <div className="mb-3 flex items-center gap-2 rounded-lg border border-border px-3 py-2">
                      <i className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-foreground" />
                      <span className="min-w-0 truncate font-mono text-[10px] text-foreground">{tunnelUrl}</span>
                    </div>
                    <div className="mb-3 flex items-center gap-1.5 font-mono text-[10px] text-muted">
                      {webhookRegistered
                        ? <><Check size={11} className="text-foreground" />webhook registered</>
                        : <><Loader2 size={11} />webhook not yet registered</>}
                    </div>
                    {!webhookRegistered && (
                      <p className="mb-3 rounded-lg border border-border px-3 py-2 text-[11px] leading-relaxed text-muted">
                        Make sure the bot token is saved above (token / api key), then click <b className="text-foreground">register webhook</b>.
                      </p>
                    )}
                    <div className="flex gap-2">
                      {!webhookRegistered && (
                        <Button variant="primary" className="flex-1" icon={<Rocket size={13} />} onClick={onRegisterWebhook} disabled={tunnelBusy}>register webhook</Button>
                      )}
                      <Button className="flex-1" icon={<X size={12} />} onClick={onStopTunnel} disabled={tunnelBusy}>stop tunnel</Button>
                    </div>
                  </>
                ) : (
                  <Button variant="primary" className="w-full" icon={<Rocket size={13} />} onClick={onExposeTelegram} disabled={tunnelBusy}>
                    {tunnelBusy ? 'working…' : 'expose & register webhook'}
                  </Button>
                )}

                {tunnelSteps.length > 0 && (
                  <div className="mt-3 rounded-lg border border-border p-2.5">
                    {tunnelSteps.map((s, i) => {
                      const failed = stepFailed(s);
                      const current = tunnelBusy && i === tunnelSteps.length - 1;
                      return (
                        <div key={i} className="flex items-start gap-2 py-0.5 font-mono text-[10px] leading-relaxed">
                          <span className={`mt-px shrink-0 ${failed ? 'text-danger' : 'text-muted'}`}>
                            {failed ? <X size={10} /> : current ? <Loader2 size={10} className="animate-spin" /> : <Check size={10} />}
                          </span>
                          <span className={failed ? 'text-danger' : 'text-muted'}>{cleanStep(s)}</span>
                        </div>
                      );
                    })}
                  </div>
                )}
                <p className="mt-3 mb-0 font-mono text-[10px] leading-relaxed text-muted">Requires <code className="font-mono">cloudflared</code> (auto-downloaded if missing). While the tunnel is active, long-polling pauses.</p>
              </div>

              {/* Own domain / named tunnel option */}
              <div className="mt-3 rounded-xl border border-border bg-background p-4">
                <span className={GROUP_LABEL_CLS}>use your own domain</span>
                <p className="mt-1.5 mb-3 text-[12px] leading-[1.65] text-muted">
                  Have a Cloudflare account and a domain? Set up a named tunnel in Cloudflare (pointing at <code className="font-mono">http://127.0.0.1:14789</code>) and enter its public HTTPS URL below. Telegram will send to your domain instead of a random trycloudflare URL.
                </p>
                <input
                  value={customUrl}
                  onChange={(e) => setCustomUrl(e.target.value)}
                  placeholder="https://bot.yourdomain.com"
                  className={`${INPUT_CLS} mb-2`}
                />
                <Button className="w-full" icon={<Globe size={12} />} onClick={onRegisterCustomUrl} disabled={customBusy || !customUrl.trim()}>
                  {customBusy ? 'registering…' : 'register webhook to my domain'}
                </Button>
              </div>
            </>
          )}
        </Modal>
      )}
    </>
  );
}
