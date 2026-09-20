import { useEffect, useState } from 'react';
import { Download, RefreshCw, RotateCcw, Settings, Sparkles, Trash2 } from 'lucide-react';
import type { ModelConfig } from '../../types';
import { ModelsView } from './ModelsView';
import { UpdatePrompt } from '../ui/UpdatePrompt';
import { Dropdown } from '../ui/Dropdown';
import { toast } from '../../hooks/useToast';
import { appVersion, checkForUpdate, type UpdateInfo } from '../../runtime';
import { NAV_SECTIONS, useNavLabels } from '../../hooks/useNavLabels';
import { useDictation } from '../../hooks/useDictation';

type SettingsTab = 'general' | 'models';

const TABS: { key: SettingsTab; label: string }[] = [
  { key: 'general', label: 'General' },
  { key: 'models', label: 'Models' },
];

const tabBtn = (active: boolean) =>
  `flex cursor-pointer items-center gap-1 rounded-[10px] border px-2.5 py-1.5 text-[11px] capitalize ${active ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`;

const formatSize = (bytes: number) => `${Math.round(bytes / 1_048_576)} MB`;

export function SettingsView({ models, onRerunOnboarding, onAddModel, onEditModel, onDeleteModel }: {
  models: ModelConfig[]; onRerunOnboarding: () => void;
  onAddModel: () => void; onEditModel: (m: ModelConfig) => void; onDeleteModel: (id: string) => Promise<void>;
}) {
  const [tab, setTab] = useState<SettingsTab>('general');
  const [version, setVersion] = useState('');
  const [checking, setChecking] = useState(false);
  const [found, setFound] = useState<UpdateInfo | null>(null);

  const navLabels = useNavLabels((s) => s.labels);
  const setNavLabel = useNavLabels((s) => s.setLabel);
  const resetNavLabels = useNavLabels((s) => s.reset);

  const dictationModels = useDictation((s) => s.models);
  const dictationModelId = useDictation((s) => s.modelId);
  const setDictationModelId = useDictation((s) => s.setModelId);
  const loadDictationModels = useDictation((s) => s.loadModels);
  const dictationAvailable = useDictation((s) => s.available);
  const dictationProgress = useDictation((s) => s.downloading);
  const dictationError = useDictation((s) => s.error);
  const [downloadingModel, setDownloadingModel] = useState<string | null>(null);

  useEffect(() => { appVersion().then(setVersion); }, []);
  useEffect(() => { void loadDictationModels(); }, [loadDictationModels]);

  const selectedModel = dictationModels.find((m) => m.id === dictationModelId);

  const startDictationDownload = async (id: string) => {
    setDownloadingModel(id);
    try {
      await useDictation.getState().download(id);
      toast('Speech model ready.', 'success');
    } catch (e) {
      toast(typeof e === 'string' ? e : 'The model download failed.', 'error');
    } finally {
      setDownloadingModel(null);
    }
  };

  const checkUpdates = async () => {
    setChecking(true);
    try {
      const u = await checkForUpdate();
      if (u) setFound(u);
      else toast('You are on the latest version.', 'success');
    } catch (e) {
      toast(typeof e === 'string' ? e : 'Could not check for updates.', 'error');
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <header className="mb-5 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          <Settings size={14} className="text-[var(--green)]" />
          <span className="font-mono text-[11px] uppercase tracking-[1px] text-text">Settings</span>
        </div>
        <div className="flex items-center gap-1">
          {TABS.map((t) => (
            <button key={t.key} className={tabBtn(tab === t.key)} onClick={() => setTab(t.key)}>{t.label}</button>
          ))}
        </div>
      </header>

      <div className={tab === 'general' ? '' : 'hidden'}>
        <div className="max-w-[700px] rounded-[16px] border border-line bg-panel p-[22px]">
          <h3 className="mb-[5px] text-[14px]">Theme</h3>
          <p className="mb-6 text-[12px] leading-[1.7] text-muted">Local-first agent workspace. Runs are recorded locally; agents are isolated per home directory.</p>
          <div className="mb-[31px] flex gap-[13px]">
            {(['dark', 'light', 'cyber'] as const).map((t) => (
              <button key={t} className={`w-[130px] cursor-pointer rounded-[10px] border border-line bg-transparent p-2 text-left hover:border-mid ${t === 'dark' ? 'border-[var(--green)]' : ''}`} onClick={() => document.documentElement.setAttribute('data-theme', t)}>
                <span className={`mb-[7px] block h-[43px] rounded bg-[#f3f3f4] ${t === 'dark' ? 'bg-[#19191f]' : t === 'cyber' ? 'bg-[linear-gradient(135deg,#091020,#243267)]' : ''}`} />
                <b className="text-[11px]">{t}</b>
              </button>
            ))}
          </div>
          <p className="text-[12px] text-muted">Everything runs locally — your agents, tools, and data stay on this machine.</p>
        </div>

        <div className="mt-4 max-w-[700px] rounded-[16px] border border-line bg-panel p-[22px]">
          <h3 className="mb-[5px] text-[14px]">Updates</h3>
          <p className="mb-4 text-[12px] leading-[1.7] text-muted">
            {version ? `Installed version ${version}. ` : ''}Signed builds update in place from GitHub Releases.
          </p>
          <button className="primary" onClick={checkUpdates} disabled={checking}>
            <RefreshCw size={13} className={checking ? 'animate-spin' : ''} />{checking ? 'Checking…' : 'Check for updates'}
          </button>
        </div>

        <div className="mt-4 max-w-[700px] rounded-[16px] border border-line bg-panel p-[22px]">
          <h3 className="mb-[5px] text-[14px]">Dictation</h3>
          <p className="mb-4 text-[12px] leading-[1.7] text-muted">
            Transcribe speech into the chat box with Whisper. Everything runs on this machine; the model is downloaded once and works offline.
          </p>
          <label className="mb-1.5 block text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">MODEL</label>
          <Dropdown
            value={dictationModelId}
            options={dictationModels.map((m) => ({ value: m.id, label: `${m.label} · ${formatSize(m.sizeBytes)}` }))}
            onChange={setDictationModelId}
          />
          <div className="mt-4 flex flex-wrap items-center gap-2">
            {downloadingModel ? (
              <button className="primary" disabled><RefreshCw size={13} className="animate-spin" />Downloading…</button>
            ) : selectedModel?.downloaded ? (
              <button className="secondary" onClick={() => void useDictation.getState().remove(dictationModelId)}><Trash2 size={12} />Remove model</button>
            ) : (
              <button className="primary" onClick={() => void startDictationDownload(dictationModelId)}><Download size={13} />Download model</button>
            )}
            {downloadingModel && dictationProgress && (
              <span className="font-mono text-[11px] text-muted">
                {dictationProgress.total > 0
                  ? `${Math.round((dictationProgress.received / dictationProgress.total) * 100)}% · ${formatSize(dictationProgress.received)} of ${formatSize(dictationProgress.total)}`
                  : 'Starting…'}
              </span>
            )}
          </div>
          <p className="mt-3 text-[11px] text-muted">
            {selectedModel
              ? (selectedModel.downloaded ? 'Ready to use.' : `Not downloaded yet — about ${formatSize(selectedModel.sizeBytes)}.`)
              : 'Loading the model list…'}
            {dictationAvailable ? '' : ' No microphone was detected on this machine.'}
          </p>
          {dictationError && <p className="mt-1 text-[11px] text-[#f87171]">{dictationError}</p>}
        </div>

        <div className="mt-4 max-w-[700px] rounded-[16px] border border-line bg-panel p-[22px]">
          <h3 className="mb-[5px] text-[14px]">Navigation</h3>
          <p className="mb-4 text-[12px] leading-[1.7] text-muted">Rename the navigation sections. Leave a field blank to restore the default.</p>
          <div className="grid gap-2">
            {NAV_SECTIONS.map((s) => (
              <label key={s.key} className="flex items-center gap-3">
                <span className="w-[92px] shrink-0 font-mono text-[11px] uppercase tracking-[0.08em] text-muted">{s.label}</span>
                <input
                  value={navLabels[s.key] ?? ''}
                  onChange={(e) => setNavLabel(s.key, e.target.value)}
                  placeholder={s.label}
                  className="min-w-0 flex-1 rounded-md border border-line bg-panel2 px-3 py-1.5 text-[12.5px] text-text outline-none focus:border-mid"
                />
              </label>
            ))}
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <button className="secondary" onClick={resetNavLabels}><RotateCcw size={12} />Reset names</button>
            <button className="secondary" onClick={onRerunOnboarding}><Sparkles size={12} />Re-run onboarding</button>
          </div>
        </div>
      </div>

      {found && <UpdatePrompt info={found} onDismiss={() => setFound(null)} />}

      <div className={tab === 'models' ? '' : 'hidden'}>
        <ModelsView embedded models={models} onAdd={onAddModel} onEdit={onEditModel} onDelete={onDeleteModel} />
      </div>
    </div>
  );
}
