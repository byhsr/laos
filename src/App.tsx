import { useEffect, useMemo, useState } from 'react';
import type { Agent, DrawerForm, View } from './types';
import { emptyModel, emptySkill, emptyTool } from './types';
import { useAgentsStore } from './hooks/useAgents';
import { useRunsStore } from './hooks/useRuns';
import { useModelsStore } from './hooks/useModels';
import { useToolsStore } from './hooks/useTools';
import { useSkillsStore } from './hooks/useSkills';
import { useWorkflowsStore } from './hooks/useWorkflows';
import { useWorkspaceStore } from './hooks/useWorkspace';
import { useIntegrationsStore } from './hooks/useIntegrations';
import { Topbar } from './components/Topbar';
import { Sidebar } from './components/Sidebar';
import { AgentWindow } from './components/AgentWindow';
import { HomeView, type HomeTab } from './components/views/HomeView';
import { AgentsView } from './components/views/AgentsView';
import { CanvasView } from './components/views/CanvasView';
import { ToolFormDrawer } from './components/views/ToolsView';
import { ModelFormDrawer } from './components/views/ModelsView';
import { SettingsView } from './components/views/SettingsView';
import { ManagerView } from './components/views/ManagerView';
import { TasksView } from './components/views/TasksView';
import { TelegramView } from './components/views/TelegramView';
import { RunsConsole } from './components/views/RunsConsole';
import { WorkshopView } from './components/views/WorkshopView';
import { SkillFormDrawer } from './components/views/SkillsView';
import { Toaster } from './components/ui/Toaster';
import { UpdatePrompt } from './components/ui/UpdatePrompt';
import { toast } from './hooks/useToast';
import { checkForUpdate, type UpdateInfo } from './runtime';
import { Plus } from 'lucide-react';

export default function App() {
  const agents = useAgentsStore((s) => s.agents);
  const setAgents = useAgentsStore((s) => s.setAgents);
  const persistAgent = useAgentsStore((s) => s.persistAgent);
  const deleteAgent = useAgentsStore((s) => s.deleteAgent);
  const createAgent = useAgentsStore((s) => s.createAgent);
  const loadAgents = useAgentsStore((s) => s.loadAgents);
  const runs = useRunsStore((s) => s.runs);
  const handleRun = useRunsStore((s) => s.handleRun);
  const clearRuns = useRunsStore((s) => s.clearRuns);
  const models = useModelsStore((s) => s.models);
  const saveModel = useModelsStore((s) => s.saveModel);
  const deleteModel = useModelsStore((s) => s.deleteModel);
  const loadModels = useModelsStore((s) => s.loadModels);
  const tools = useToolsStore((s) => s.tools);
  const setTools = useToolsStore((s) => s.setTools);
  const saveTool = useToolsStore((s) => s.saveTool);
  const deleteTool = useToolsStore((s) => s.deleteTool);
  const loadTools = useToolsStore((s) => s.loadTools);
  const skills = useSkillsStore((s) => s.skills);
  const saveSkill = useSkillsStore((s) => s.saveSkill);
  const deleteSkill = useSkillsStore((s) => s.deleteSkill);
  const loadSkills = useSkillsStore((s) => s.loadSkills);
  const integrations = useIntegrationsStore((s) => s.integrations);
  const loadIntegrations = useIntegrationsStore((s) => s.loadIntegrations);
  const loadWorkspace = useWorkspaceStore((s) => s.loadWorkspace);
  const workflows = useWorkflowsStore((s) => s.workflows);
  const saveWorkflow = useWorkflowsStore((s) => s.saveWorkflow);
  const deleteWorkflow = useWorkflowsStore((s) => s.deleteWorkflow);
  const runWorkflow = useWorkflowsStore((s) => s.runWorkflow);
  const loadWorkflows = useWorkflowsStore((s) => s.loadWorkflows);
  const loadRuns = useRunsStore((s) => s.loadRuns);

  useEffect(() => {
    loadAgents(); loadModels(); loadTools(); loadSkills(); loadWorkspace(); loadWorkflows(); loadIntegrations(); loadRuns();
  }, [loadAgents, loadModels, loadTools, loadSkills, loadWorkspace, loadWorkflows, loadIntegrations, loadRuns]);

  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);

  // Check for a new release shortly after launch — never blocks startup, and an
  // offline/failed check is ignored.
  useEffect(() => {
    const t = setTimeout(() => {
      checkForUpdate().then((u) => { if (u) setUpdate(u); }).catch(() => {});
    }, 4000);
    return () => clearTimeout(t);
  }, []);
  const [view, setView] = useState<View>('home');
  const [homeTab, setHomeTab] = useState<HomeTab>('overview');
  const [drawerForm, setDrawerForm] = useState<DrawerForm>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [workflowToOpen, setWorkflowToOpen] = useState<string | null>(null);

  const selectedAgent = useMemo(() => agents.find((a) => a.id === selectedAgentId) ?? null, [agents, selectedAgentId]);

  const openAgent = (id: string) => {
    // The Manager is a root-level view; opening it goes to the Manager tab.
    if (agents.find((a) => a.id === id)?.isManager) {
      setView('manager');
      return;
    }
    setSelectedAgentId(id);
    setView('agent');
  };

  const addAgent = async () => {
    const agent = await createAgent({});
    setSelectedAgentId(agent.id);
    setView('agent');
    toast(`Agent "${agent.name}" created`, 'success');
  };

  return (
    <div className="relative flex h-screen flex-col overflow-hidden">
      <div className="ambient-layer" aria-hidden />
      <Topbar
        collapsed={sidebarCollapsed}
        onToggleSidebar={() => setSidebarCollapsed((c) => !c)}
        view={view}
        setView={setView}
        onNewAgent={addAgent}
        agents={agents}
        runs={runs}
        onOpenGraph={() => { setView('home'); setHomeTab('graph'); }}
        graphActive={view === 'home' && homeTab === 'graph'}
      />
      <div className="relative z-10 flex min-h-0 flex-1">
        <Sidebar
          agents={agents}
          view={view}
          selectedAgentId={selectedAgentId}
          onOpen={openAgent}
          onBrowse={() => setView('agents')}
          onHome={() => setView('home')}
          collapsed={sidebarCollapsed}
        />
        <main className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-10 py-9">
          {/* Views stay mounted; hidden ones keep their live state (chats, streaming). */}
          <div className={`h-full ${view === 'home' ? '' : 'hidden'}`}><HomeView agents={agents} tools={tools} skills={skills} integrations={integrations} runs={runs} workflows={workflows} onOpen={openAgent} onCreate={addAgent} onOpenWorkflow={(id) => { setView('workflows'); setWorkflowToOpen(id); }} onSaveAgent={persistAgent} onSaveWorkflow={saveWorkflow} tab={homeTab} onTabChange={setHomeTab} /></div>
          <div className={view === 'agents' ? '' : 'hidden'}><AgentsView agents={agents} tools={tools} onOpen={openAgent} onCreate={addAgent} onDelete={async (id) => { await deleteAgent(id); if (selectedAgentId === id) setSelectedAgentId(null); toast('Agent deleted', 'success'); }} /></div>
          <div className={view === 'workflows' ? '' : 'hidden'}>
            <CanvasView
              agents={agents} tools={tools} workflows={workflows} integrations={integrations}
              initialWorkflowId={workflowToOpen}
              onInitialWorkflowConsumed={() => setWorkflowToOpen(null)}
              onSaveWorkflow={saveWorkflow}
              onDeleteWorkflow={deleteWorkflow}
              onRunWorkflow={runWorkflow}
            />
          </div>
          <div className={view === 'manager' ? '' : 'hidden'}><ManagerView agents={agents} integrations={integrations} models={models} /></div>
          <div className={view === 'tasks' ? '' : 'hidden'}><TasksView agents={agents} /></div>
          <div className={view === 'telegram' ? '' : 'hidden'}><TelegramView /></div>
          <div className={view === 'runs' ? '' : 'hidden'}>
            <RunsConsole runs={runs} agents={agents} onOpenAgent={openAgent} onClear={clearRuns} />
          </div>
          <div className={view === 'workshop' ? '' : 'hidden'}>
            <WorkshopView
              skills={skills} tools={tools} integrations={integrations}
              onAddSkill={() => setDrawerForm({ kind: 'skill', editing: emptySkill(), isNew: true })}
              onEditSkill={(s) => setDrawerForm({ kind: 'skill', editing: s, isNew: false })}
              onDeleteSkill={async (id) => { await deleteSkill(id); setAgents((prev) => prev.map((a) => ({ ...a, skillIds: a.skillIds.filter((s) => s !== id) }))); toast('Skill deleted', 'success'); }}
              onAddTool={() => setDrawerForm({ kind: 'tool', editing: emptyTool(), isNew: true })}
              onEditTool={(t) => setDrawerForm({ kind: 'tool', editing: t, isNew: false })}
              onDeleteTool={async (id) => { await deleteTool(id); setTools((prev) => prev.filter((p) => p.id !== id)); setAgents((prev) => prev.map((a) => ({ ...a, toolIds: a.toolIds.filter((t) => t !== id) }))); toast('Tool deleted', 'success'); }}
            />
          </div>
          <div className={view === 'settings' ? '' : 'hidden'}>
            <SettingsView
              models={models}
              onAddModel={() => setDrawerForm({ kind: 'model', editing: emptyModel(), isNew: true })}
              onEditModel={(m) => setDrawerForm({ kind: 'model', editing: m, isNew: false })}
              onDeleteModel={deleteModel}
            />
          </div>
          {selectedAgent && (
            <div className={view === 'agent' ? '' : 'hidden'}><AgentWindow key={selectedAgent.id} agent={selectedAgent} tools={tools} skills={skills} models={models} integrations={integrations} runs={runs} onBack={() => setView('agents')} onSave={persistAgent} onDelete={async (id) => { await deleteAgent(id); setSelectedAgentId(null); setView('agents'); }} onRun={handleRun} /></div>
          )}
        </main>
        {drawerForm && drawerForm.kind === 'tool' && (
          <ToolFormDrawer
            editing={drawerForm.editing} isNew={drawerForm.isNew} integrations={integrations}
            onClose={() => setDrawerForm(null)}
            onSave={async (t) => { await saveTool(t); setDrawerForm(null); toast('Tool saved', 'success'); }}
          />
        )}
        {drawerForm && drawerForm.kind === 'model' && (
          <ModelFormDrawer
            editing={drawerForm.editing} isNew={drawerForm.isNew}
            onClose={() => setDrawerForm(null)}
            onSave={async (m) => { await saveModel(m); setDrawerForm(null); toast('Model saved', 'success'); }}
          />
        )}
        {drawerForm && drawerForm.kind === 'skill' && (
          <SkillFormDrawer
            editing={drawerForm.editing} isNew={drawerForm.isNew}
            onClose={() => setDrawerForm(null)}
            onSave={async (s) => { await saveSkill(s); setDrawerForm(null); toast('Skill saved', 'success'); }}
          />
        )}
      </div>
      <Toaster />
      {update && <UpdatePrompt info={update} onDismiss={() => setUpdate(null)} />}
    </div>
  );
}
