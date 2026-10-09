import { useState } from 'react';
import type { Integration, Project, Script, Skill, Tool } from '../../types';
import { SkillsView } from './SkillsView';
import { ScriptsView } from './ScriptsView';
import { ProjectsView } from './ProjectsView';
import { ToolsView } from './ToolsView';
import { IntegrationsView } from './IntegrationsView';
import { KnowledgeBaseView } from './KnowledgeBaseView';
import { tabCls } from '../ui/tabs';

type WorkshopTab = 'skills' | 'scripts' | 'projects' | 'tools' | 'integrations' | 'knowledge';

const TABS: { key: WorkshopTab; label: string }[] = [
  { key: 'skills', label: 'skills' },
  { key: 'scripts', label: 'scripts' },
  { key: 'projects', label: 'projects' },
  { key: 'tools', label: 'tools' },
  { key: 'integrations', label: 'integrations' },
  { key: 'knowledge', label: 'knowledge' },
];

export function WorkshopView({
  skills, scripts, projects, activeProjectId, tools, integrations,
  onAddSkill, onEditSkill, onDeleteSkill,
  onAddScript, onEditScript, onDeleteScript,
  onAddProject, onEditProject, onDeleteProject, onSetActiveProject,
  onAddTool, onEditTool, onDeleteTool,
}: {
  skills: Skill[]; scripts: Script[]; projects: Project[]; activeProjectId: string; tools: Tool[]; integrations: Integration[];
  onAddSkill: () => void; onEditSkill: (s: Skill) => void; onDeleteSkill: (id: string) => Promise<void>;
  onAddScript: () => void; onEditScript: (s: Script) => void; onDeleteScript: (id: string) => Promise<void>;
  onAddProject: () => void; onEditProject: (p: Project) => void; onDeleteProject: (id: string) => Promise<void>; onSetActiveProject: (id: string) => void;
  onAddTool: () => void; onEditTool: (t: Tool) => void; onDeleteTool: (id: string) => Promise<void>;
}) {
  const [tab, setTab] = useState<WorkshopTab>('skills');

  return (
    <div className="flex h-full flex-col">
      {/* No view title — the header row carries only the tabs. */}
      <div className="mb-4 flex shrink-0 flex-wrap items-center justify-end gap-1">
        {TABS.map((t) => (
          <button key={t.key} className={tabCls(tab === t.key)} onClick={() => setTab(t.key)}>{t.label}</button>
        ))}
      </div>

      {/* Sub-views stay mounted so their local state survives tab switches. */}
      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'skills' ? '' : 'hidden'}`}>
        <SkillsView skills={skills} onAdd={onAddSkill} onEdit={onEditSkill} onDelete={onDeleteSkill} />
      </div>
      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'scripts' ? '' : 'hidden'}`}>
        <ScriptsView scripts={scripts} onAdd={onAddScript} onEdit={onEditScript} onDelete={onDeleteScript} />
      </div>
      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'projects' ? '' : 'hidden'}`}>
        <ProjectsView projects={projects} activeId={activeProjectId} onAdd={onAddProject} onEdit={onEditProject} onDelete={onDeleteProject} onSetActive={onSetActiveProject} />
      </div>
      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'tools' ? '' : 'hidden'}`}>
        <ToolsView tools={tools} integrations={integrations} onAdd={onAddTool} onEdit={onEditTool} onDelete={onDeleteTool} />
      </div>
      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'integrations' ? '' : 'hidden'}`}>
        <IntegrationsView integrations={integrations} />
      </div>
      <div className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto ${tab === 'knowledge' ? '' : 'hidden'}`}>
        <KnowledgeBaseView />
      </div>
    </div>
  );
}
