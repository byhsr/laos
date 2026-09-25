import { useEffect, useState } from 'react';
import { RefreshCw, RotateCcw, Sparkles } from 'lucide-react';
import type { ModelConfig } from '../../types';
import { ModelsView } from './ModelsView';
import { UpdatePrompt } from '../ui/UpdatePrompt';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { FIELD_LABEL_CLS, GROUP_LABEL_CLS, INPUT_CLS } from '../ui/Input';
import { tabCls } from '../ui/tabs';
import { toast } from '../../hooks/useToast';
import { appVersion, checkForUpdate, type UpdateInfo } from '../../runtime';
import { NAV_SECTIONS, useNavLabels } from '../../hooks/useNavLabels';
import { THEMES, activeTheme, applyTheme, type ThemeKey } from '../../theme';

type SettingsTab = 'general' | 'models';

const TABS: { key: SettingsTab; label: string }[] = [
  { key: 'general', label: 'general' },
  { key: 'models', label: 'models' },
];

export function SettingsView({ models, onRerunOnboarding, onAddModel, onEditModel, onDeleteModel }: {
  models: ModelConfig[]; onRerunOnboarding: () => void;
  onAddModel: () => void; onEditModel: (m: ModelConfig) => void; onDeleteModel: (id: string) => Promise<void>;
}) {
  const [tab, setTab] = useState<SettingsTab>('general');
  const [version, setVersion] = useState('');
  const [checking, setChecking] = useState(false);
  const [found, setFound] = useState<UpdateInfo | null>(null);
  const [theme, setTheme] = useState<ThemeKey>(activeTheme);

  const navLabels = useNavLabels((s) => s.labels);
  const setNavLabel = useNavLabels((s) => s.setLabel);
  const resetNavLabels = useNavLabels((s) => s.reset);

  useEffect(() => { appVersion().then(setVersion); }, []);

  const pickTheme = (key: ThemeKey) => {
    applyTheme(key);
    setTheme(key);
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
        </div>
      </div>

      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'models' ? '' : 'hidden'}`}>
        <ModelsView models={models} onAdd={onAddModel} onEdit={onEditModel} onDelete={onDeleteModel} />
      </div>

      {found && <UpdatePrompt info={found} onDismiss={() => setFound(null)} />}
    </div>
  );
}
