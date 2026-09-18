import { useState } from 'react';
import { Boxes } from 'lucide-react';
import type { Agent, Integration, Run, Skill, Tool } from '../../types';
import { SkillsView } from './SkillsView';
import { ToolsView } from './ToolsView';
import { IntegrationsView } from './IntegrationsView';
import { KnowledgeBaseView } from './KnowledgeBaseView';
import { RunsConsole } from './RunsConsole';

type WorkshopTab = 'skills' | 'tools' | 'integrations' | 'knowledge' | 'runs';

const TABS: { key: WorkshopTab; label: string }[] = [
  { key: 'skills', label: 'Skills' },
  { key: 'tools', label: 'Tools' },
  { key: 'integrations', label: 'Integrations' },
  { key: 'knowledge', label: 'Knowledge' },
  { key: 'runs', label: 'Runs' },
];

const tabBtn = (active: boolean) =>
  `flex cursor-pointer items-center gap-1 rounded-[10px] border px-2.5 py-1.5 text-[11px] capitalize ${active ? 'border-dotted border-mid bg-panel2 text-text' : 'border-transparent bg-none text-muted hover:text-text'}`;

export function WorkshopView({
  skills, tools, integrations, runs, agents,
  onAddSkill, onEditSkill, onDeleteSkill,
  onAddTool, onEditTool, onDeleteTool,
  onOpenAgent, onClearRuns,
}: {
  skills: Skill[]; tools: Tool[]; integrations: Integration[]; runs: Run[]; agents: Agent[];
  onAddSkill: () => void; onEditSkill: (s: Skill) => void; onDeleteSkill: (id: string) => Promise<void>;
  onAddTool: () => void; onEditTool: (t: Tool) => void; onDeleteTool: (id: string) => Promise<void>;
  onOpenAgent: (id: string) => void; onClearRuns: () => void;
}) {
  const [tab, setTab] = useState<WorkshopTab>('skills');

  return (
    <div className="flex h-full flex-col">
      <header className="mb-5 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          <Boxes size={14} className="text-[var(--green)]" />
          <span className="font-mono text-[11px] uppercase tracking-[1px] text-text">Workshop</span>
        </div>
        <div className="flex items-center gap-1">
          {TABS.map((t) => (
            <button key={t.key} className={tabBtn(tab === t.key)} onClick={() => setTab(t.key)}>{t.label}</button>
          ))}
        </div>
      </header>

      {/* Sub-views stay mounted so their local state survives tab switches. */}
      <div className={tab === 'skills' ? '' : 'hidden'}>
        <SkillsView embedded skills={skills} onAdd={onAddSkill} onEdit={onEditSkill} onDelete={onDeleteSkill} />
      </div>
      <div className={tab === 'tools' ? '' : 'hidden'}>
        <ToolsView embedded tools={tools} integrations={integrations} onAdd={onAddTool} onEdit={onEditTool} onDelete={onDeleteTool} />
      </div>
      <div className={tab === 'integrations' ? '' : 'hidden'}>
        <IntegrationsView embedded integrations={integrations} />
      </div>
      <div className={tab === 'knowledge' ? '' : 'hidden'}>
        <KnowledgeBaseView embedded />
      </div>
      <div className={tab === 'runs' ? '' : 'hidden'}>
        <RunsConsole embedded runs={runs} agents={agents} onOpenAgent={onOpenAgent} onClear={onClearRuns} />
      </div>
    </div>
  );
}
