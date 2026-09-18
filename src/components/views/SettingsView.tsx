import { useState } from 'react';
import { Settings } from 'lucide-react';
import type { ModelConfig } from '../../types';
import { ModelsView } from './ModelsView';

type SettingsTab = 'general' | 'models';

const TABS: { key: SettingsTab; label: string }[] = [
  { key: 'general', label: 'General' },
  { key: 'models', label: 'Models' },
];

const tabBtn = (active: boolean) =>
  `flex cursor-pointer items-center gap-1 rounded-[10px] border px-2.5 py-1.5 text-[11px] capitalize ${active ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`;

export function SettingsView({ models, onAddModel, onEditModel, onDeleteModel }: {
  models: ModelConfig[]; onAddModel: () => void; onEditModel: (m: ModelConfig) => void; onDeleteModel: (id: string) => Promise<void>;
}) {
  const [tab, setTab] = useState<SettingsTab>('general');

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
      </div>

      <div className={tab === 'models' ? '' : 'hidden'}>
        <ModelsView embedded models={models} onAdd={onAddModel} onEdit={onEditModel} onDelete={onDeleteModel} />
      </div>
    </div>
  );
}
