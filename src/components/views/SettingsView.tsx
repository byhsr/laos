import { useEffect, useRef, useState, type ChangeEvent } from 'react';
import { RefreshCw, RotateCcw, Sparkles } from 'lucide-react';
import type { ModelConfig, View } from '../../types';
import { ModelsView } from './ModelsView';
import { UpdatePrompt } from '../ui/UpdatePrompt';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS } from '../ui/Input';
import { Select } from '../ui/Select';
import { tabCls } from '../ui/tabs';
import { toast } from '../../hooks/useToast';
import { appVersion, checkForUpdate, getAppSettings, getIdentity, setIdentity, setRunInBackground, setLaunchAtLogin, setAutonomy, type Identity, type UpdateInfo } from '../../runtime';
import { Checkbox } from '../ui/Checkbox';
import { NAV_SECTIONS, navLabel, useNavLabels } from '../../hooks/useNavLabels';
import { useSidebarTabs } from '../../hooks/useSidebarTabs';
import { RAIL_KEYS } from '../Rail';
import { THEMES, activeTheme, applyTheme, type ThemeKey } from '../../theme';
import { useGlass, GLASS_BLUR_STOPS, GLASS_TINT_STOPS, readBackgroundImage } from '../../hooks/useGlass';
import { useThemeColors, COLOR_TOKENS, type ColorToken } from '../../hooks/useThemeColors';
import { Slider } from '../ui/Slider';
import { ColorField } from '../ui/ColorField';
import { Section } from '../ui/Section';

type SettingsTab = 'general' | 'models';

const TABS: { key: SettingsTab; label: string }[] = [
  { key: 'general', label: 'general' },
  { key: 'models', label: 'models' },
];

// Only the sections that actually live in the rail can be hidden from it.
const RAIL_SECTIONS = NAV_SECTIONS.filter((s) => RAIL_KEYS.includes(s.key));

export function SettingsView({ models, onRerunOnboarding, onAddModel, onEditModel, onDeleteModel, onNavigate }: {
  models: ModelConfig[]; onRerunOnboarding: () => void;
  onAddModel: () => void; onEditModel: (m: ModelConfig) => void; onDeleteModel: (id: string) => Promise<void>;
  onNavigate: (v: View) => void;
}) {
  const [tab, setTab] = useState<SettingsTab>('general');
  const [version, setVersion] = useState('');
  const [checking, setChecking] = useState(false);
  const [found, setFound] = useState<UpdateInfo | null>(null);
  const [theme, setTheme] = useState<ThemeKey>(activeTheme);
  const [bg, setBg] = useState(true);
  const [login, setLogin] = useState(false);
  const [autonomy, setAutonomyOn] = useState(true);
  const [who, setWho] = useState<Identity>({ userName: '', botName: '', persona: '', tone: '', timezone: '', verbosity: 0 });
  const glassImage = useGlass((s) => s.image);
  const glassBlur = useGlass((s) => s.blur);
  const glassTint = useGlass((s) => s.tint);
  const setGlassImage = useGlass((s) => s.setImage);
  const setGlassBlur = useGlass((s) => s.setBlur);
  const setGlassTint = useGlass((s) => s.setTint);
  const glassFileRef = useRef<HTMLInputElement>(null);
  const colorOverrides = useThemeColors((s) => s.overrides);
  const setThemeColor = useThemeColors((s) => s.setColor);
  const resetThemeColors = useThemeColors((s) => s.resetTheme);
  const themeColours = colorOverrides[theme] ?? {};
  const colourCount = Object.keys(themeColours).length;
  const colourSummary = colourCount ? `${colourCount} overridden` : 'theme defaults';

  const navLabels = useNavLabels((s) => s.labels);
  const setNavLabel = useNavLabels((s) => s.setLabel);
  const resetNavLabels = useNavLabels((s) => s.reset);
  const hiddenTabs = useSidebarTabs((s) => s.hidden);
  const toggleTab = useSidebarTabs((s) => s.toggle);
  const showAllTabs = useSidebarTabs((s) => s.showAll);

  useEffect(() => { appVersion().then(setVersion); }, []);
  useEffect(() => { getAppSettings().then((s) => { setBg(s.runInBackground); setLogin(s.launchAtLogin); setAutonomyOn(s.autonomy); }); }, []);
  useEffect(() => { getIdentity().then(setWho); }, []);

  const saveWho = async () => {
    try { await setIdentity(who); toast('saved', 'success'); }
    catch (e) { toast(typeof e === 'string' ? e : 'could not save', 'error'); }
  };

  const toggleBg = async (next: boolean) => {
    setBg(next);
    try {
      await setRunInBackground(next);
      toast(next ? 'keeps running in the tray' : 'closing the window will quit', 'success');
    } catch (e) { setBg(!next); toast(typeof e === 'string' ? e : 'could not save', 'error'); }
  };

  const toggleLogin = async (next: boolean) => {
    setLogin(next);
    try {
      await setLaunchAtLogin(next);
      toast(next ? 'will launch at login' : 'removed from startup', 'success');
    } catch (e) { setLogin(!next); toast(typeof e === 'string' ? e : 'could not update startup', 'error'); }
  };

  const toggleAutonomy = async (next: boolean) => {
    setAutonomyOn(next);
    try {
      await setAutonomy(next);
      toast(next ? 'autonomous — no per-action approval' : 'will ask before state-changing actions', 'success');
    } catch (e) { setAutonomyOn(!next); toast(typeof e === 'string' ? e : 'could not save', 'error'); }
  };

  const pickTheme = (key: ThemeKey) => {
    applyTheme(key);
    setTheme(key);
  };

  // The colour painted right now for a token — the theme default, or the
  // override if one is set (the swatch only falls back to this when unset).
  const resolvedColour = (token: ColorToken) =>
    getComputedStyle(document.documentElement).getPropertyValue(`--color-${token}`).trim() || 'transparent';

  const pickGlassImage = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) { toast('that is not an image', 'error'); return; }
    try { setGlassImage(await readBackgroundImage(file)); }
    catch { toast('could not read that image', 'error'); }
  };

  const checkUpdates = async () => {
    setChecking(true);
    try {
      const u = await checkForUpdate();
      if (u) setFound(u);
      else toast('you are on the latest version', 'success');
    } catch (e) {
      toast(typeof e === 'string' ? e : 'could not check for updates', 'error');
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* No view title — the header row carries only the tabs. */}
      <div className="mb-4 flex shrink-0 flex-wrap items-center justify-end gap-1">
        {TABS.map((t) => (
          <button key={t.key} className={tabCls(tab === t.key)} onClick={() => setTab(t.key)}>{t.label}</button>
        ))}
      </div>

      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'general' ? '' : 'hidden'}`}>
        <div className="grid gap-3">
          <Card className="hover:bg-surface">
            <h3 className="m-0 font-mono text-xs lowercase text-foreground">theme</h3>
            <p className="mt-1 mb-4 text-[12px] leading-[1.65] text-muted">Local-first agent workspace. Runs are recorded locally; agents are isolated per home directory.</p>
            <div className="flex flex-wrap gap-2.5">
              {THEMES.map((t) => (
                <button
                  key={t.key}
                  onClick={() => pickTheme(t.key)}
                  aria-pressed={theme === t.key}
                  className={`focus-ring w-[128px] cursor-pointer rounded-lg border p-2 text-left transition-colors ${
                    theme === t.key ? 'border-foreground/40' : 'border-border hover:border-foreground/30'
                  }`}
                >
                  {/* Swatch drawn from the theme registry — the only place the
                      non-active themes' colours exist. */}
                  <span className="mb-2 block h-10 w-full overflow-hidden rounded border" style={{ background: t.background, borderColor: t.border }}>
                    <span className="mt-2 ml-2 block h-4 w-16 rounded-sm" style={{ background: t.surface, border: `1px solid ${t.border}` }} />
                  </span>
                  <b className="font-mono text-[10px] text-muted">{t.key}</b>
                </button>
              ))}
            </div>
          </Card>

          <Card className="hover:bg-surface">
            <div className="flex items-start justify-between gap-2">
              <h3 className="m-0 font-mono text-xs lowercase text-foreground">colours</h3>
              {Object.keys(themeColours).length > 0 && (
                <button
                  type="button"
                  onClick={() => resetThemeColors(theme)}
                  className="shrink-0 cursor-pointer font-mono text-[10px] text-muted transition-colors hover:text-foreground"
                >
                  reset all
                </button>
              )}
            </div>
            <p className="mt-1 mb-1 text-[12px] leading-[1.65] text-muted">
              Recolour the <b className="font-mono text-[11px] text-foreground">{theme}</b> theme. Each theme keeps its own overrides, so paper and glass can differ — in glass a colour retints the frost rather than replacing it.
            </p>
            <Section title="custom" summary={colourSummary}>
              <div className="grid gap-1.5">
                {COLOR_TOKENS.map((t) => (
                  <ColorField
                    key={t.key}
                    label={t.label}
                    resolved={resolvedColour(t.key)}
                    value={themeColours[t.key] ?? ''}
                    onChange={(v) => setThemeColor(theme, t.key, v)}
                    onReset={() => setThemeColor(theme, t.key, '')}
                  />
                ))}
              </div>
            </Section>
          </Card>

          <Card className="hover:bg-surface">
            <h3 className="m-0 font-mono text-xs lowercase text-foreground">glass</h3>
            <p className="mt-1 mb-3.5 text-[12px] leading-[1.65] text-muted">
              How the glass theme looks — a backdrop image behind the frost, and how heavy the frost reads. Other themes ignore this.
            </p>
            <div className="grid gap-3">
              <div>
                <span className={FIELD_LABEL_CLS}>backdrop image</span>
                <div className="flex items-center gap-2">
                  <span
                    aria-hidden
                    className="h-10 w-16 shrink-0 rounded border border-border bg-background bg-cover bg-center"
                    style={glassImage ? { backgroundImage: `url("${glassImage}")` } : undefined}
                  />
                  <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-muted">
                    {glassImage ? (glassImage.startsWith('data:') ? 'uploaded image' : glassImage) : 'the built-in backdrop'}
                  </span>
                  <input ref={glassFileRef} type="file" accept="image/*" className="hidden" onChange={pickGlassImage} />
                  <Button onClick={() => glassFileRef.current?.click()}>choose</Button>
                  {glassImage && <Button onClick={() => setGlassImage('')}>clear</Button>}
                </div>
              </div>

              <div>
                <span className={FIELD_LABEL_CLS}>frost</span>
                <Slider value={String(glassBlur)} stops={GLASS_BLUR_STOPS} ariaLabel="frost" onChange={(v) => setGlassBlur(Number(v))} />
              </div>

              <div>
                <span className={FIELD_LABEL_CLS}>tint</span>
                <Slider value={String(glassTint)} stops={GLASS_TINT_STOPS} ariaLabel="tint" onChange={(v) => setGlassTint(Number(v))} />
              </div>
            </div>
          </Card>

          <Card className="hover:bg-surface">
            <h3 className="m-0 font-mono text-xs lowercase text-foreground">you &amp; your assistant</h3>
            <p className="mt-1 mb-3.5 text-[12px] leading-[1.65] text-muted">
              Who the assistant is and who it's talking to — injected into every chat prompt.
            </p>
            <div className="grid gap-2">
              <label className="flex items-center gap-3">
                <span className={`${GROUP_LABEL_CLS} w-[92px] shrink-0`}>your name</span>
                <input value={who.userName} onChange={(e) => setWho({ ...who, userName: e.target.value })} placeholder="what it calls you" className={`${INPUT_CLS} min-w-0 flex-1`} />
              </label>
              <label className="flex items-center gap-3">
                <span className={`${GROUP_LABEL_CLS} w-[92px] shrink-0`}>bot name</span>
                <input value={who.botName} onChange={(e) => setWho({ ...who, botName: e.target.value })} placeholder="what it calls itself" className={`${INPUT_CLS} min-w-0 flex-1`} />
              </label>
              <label className="flex items-center gap-3">
                <span className={`${GROUP_LABEL_CLS} w-[92px] shrink-0`}>persona</span>
                <input value={who.persona} onChange={(e) => setWho({ ...who, persona: e.target.value })} placeholder="kind, warm, a bit witty…" className={`${INPUT_CLS} min-w-0 flex-1`} />
              </label>
              <label className="flex items-center gap-3">
                <span className={`${GROUP_LABEL_CLS} w-[92px] shrink-0`}>tone</span>
                <div className="w-[140px]">
                  <Select
                    value={who.tone}
                    options={[
                      { value: '', label: 'default' },
                      { value: 'short', label: 'short' },
                      { value: 'detailed', label: 'detailed' },
                      { value: 'witty', label: 'witty' },
                    ]}
                    onChange={(v) => setWho({ ...who, tone: v })}
                  />
                </div>
              </label>
              <label className="flex items-center gap-3">
                <span className={`${GROUP_LABEL_CLS} w-[92px] shrink-0`}>timezone</span>
                <input value={who.timezone} onChange={(e) => setWho({ ...who, timezone: e.target.value })} placeholder="informational — times use the system zone" className={`${INPUT_CLS} min-w-0 flex-1`} />
              </label>
            </div>
            <div className="mt-4"><Button variant="primary" onClick={() => void saveWho()}>save</Button></div>
          </Card>

          <Card className="hover:bg-surface">
            <h3 className="m-0 font-mono text-xs lowercase text-foreground">autonomy</h3>
            <p className="mt-1 mb-3.5 text-[12px] leading-[1.65] text-muted">
              Let agents run state-changing tools (commands, file writes, delegating) without stopping for the approval popup.
            </p>
            <Checkbox checked={autonomy} onChange={toggleAutonomy} label="autonomous — don't ask before each action" />
          </Card>

          <Card className="hover:bg-surface">
            <h3 className="m-0 font-mono text-xs lowercase text-foreground">background</h3>
            <p className="mt-1 mb-3.5 text-[12px] leading-[1.65] text-muted">
              Keep the app running in the system tray so the Telegram bots keep polling when the window is closed.
            </p>
            <div className="grid gap-2.5">
              <Checkbox checked={bg} onChange={toggleBg} label="keep running in the tray (closing hides the window)" />
              <Checkbox checked={login} onChange={toggleLogin} label="launch at login" />
            </div>
          </Card>

          <Card className="hover:bg-surface">
            <h3 className="m-0 font-mono text-xs lowercase text-foreground">updates</h3>
            <p className="mt-1 mb-3.5 text-[12px] leading-[1.65] text-muted">
              {version ? `Installed version ${version}. ` : ''}Signed builds update in place from GitHub Releases.
            </p>
            <Button variant="primary" icon={<RefreshCw size={13} className={checking ? 'animate-spin' : ''} />} onClick={checkUpdates} disabled={checking}>
              {checking ? 'checking…' : 'check for updates'}
            </Button>
          </Card>

          <Card className="hover:bg-surface">
            <h3 className="m-0 font-mono text-xs lowercase text-foreground">navigation</h3>
            <p className="mt-1 mb-3.5 text-[12px] leading-[1.65] text-muted">Rename the navigation sections. Leave a field blank to restore the default.</p>
            <div className="grid gap-2">
              {NAV_SECTIONS.map((s) => (
                <label key={s.key} className="flex items-center gap-3">
                  <span className={`${GROUP_LABEL_CLS} w-[92px] shrink-0`}>{s.label}</span>
                  <input
                    value={navLabels[s.key] ?? ''}
                    onChange={(e) => setNavLabel(s.key, e.target.value)}
                    placeholder={s.label}
                    className={`${INPUT_CLS} min-w-0 flex-1`}
                  />
                </label>
              ))}
            </div>
            <div className="mt-4 flex flex-wrap items-center gap-2">
              <Button icon={<RotateCcw size={12} />} onClick={resetNavLabels}>reset names</Button>
              <Button icon={<Sparkles size={12} />} onClick={onRerunOnboarding}>re-run onboarding</Button>
            </div>
          </Card>

          <Card className="hover:bg-surface">
            <h3 className="m-0 font-mono text-xs lowercase text-foreground">sidebar</h3>
            <p className="mt-1 mb-3.5 text-[12px] leading-[1.65] text-muted">
              Which sections sit in the rail. Pick one above to jump straight there; the section you're on always stays visible.
            </p>
            <div className="mb-3 max-w-[240px]">
              <Select
                value=""
                placeholder="jump to a section…"
                options={RAIL_SECTIONS.map((s) => ({ value: s.key, label: navLabel(navLabels, s.key) }))}
                onChange={(v) => onNavigate(v as View)}
              />
            </div>
            <div className="grid gap-2">
              {RAIL_SECTIONS.map((s) => (
                <Checkbox
                  key={s.key}
                  checked={!hiddenTabs.includes(s.key)}
                  onChange={() => toggleTab(s.key)}
                  label={`${navLabel(navLabels, s.key)} — show in sidebar`}
                />
              ))}
            </div>
            {hiddenTabs.length > 0 && (
              <div className="mt-4"><Button onClick={showAllTabs}>show all</Button></div>
            )}
          </Card>
        </div>
      </div>

      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'models' ? '' : 'hidden'}`}>
        <ModelsView models={models} onAdd={onAddModel} onEdit={onEditModel} onDelete={onDeleteModel} />
      </div>

      {found && <UpdatePrompt info={found} onDismiss={() => setFound(null)} />}
    </div>
  );
}
