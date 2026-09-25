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
import { Rail } from './components/Rail';
import { Sidebar } from './components/Sidebar';
import { AgentWindow } from './components/AgentWindow';
import { HomeView } from './components/views/HomeView';
import { GraphView } from './components/views/GraphView';
import { AgentsView } from './components/views/AgentsView';
import { CanvasView } from './components/views/CanvasView';
import { ToolFormDrawer } from './components/views/ToolsView';
import { ModelFormDrawer } from './components/views/ModelsView';
import { SettingsView } from './components/views/SettingsView';
import { ManagerView } from './components/views/ManagerView';
import { TasksView } from './components/views/TasksView';
import { TelegramView } from './components/views/TelegramView';
import { RunsConsole } from './components/views/RunsConsole';
import { Onboarding, type OnboardingPatch } from './components/Onboarding';
import { WorkshopView } from './components/views/WorkshopView';
import { SkillFormDrawer } from './components/views/SkillsView';
import { Toaster } from './components/ui/Toaster';
import { UpdatePrompt } from './components/ui/UpdatePrompt';
import { toast } from './hooks/useToast';
import { checkForUpdate, type UpdateInfo } from './runtime';

// Every view lives in a pane that scrolls internally — the shell itself never
// scrolls, so there is never a double scrollbar or a page that jumps.
const pane = (active: boolean) =>
  `h-full overflow-x-hidden overflow-y-auto p-4 ${active ? '' : 'hidden'}`;

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
  const [showOnboarding, setShowOnboarding] = useState(() => localStorage.getItem('laos.onboarded') !== '1');

  // Check for a new release shortly after launch — never blocks startup, and an
  // offline/failed check is ignored.
  useEffect(() => {
    const t = setTimeout(() => {
      checkForUpdate().then((u) => { if (u) setUpdate(u); }).catch(() => {});
    }, 4000);
    return () => clearTimeout(t);
  }, []);
  const [view, setView] = useState<View>('home');
  const [drawerForm, setDrawerForm] = useState<DrawerForm>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [workflowToOpen, setWorkflowToOpen] = useState<string | null>(null);

  const selectedAgent = useMemo(() => agents.find((a) => a.id === selectedAgentId) ?? null, [agents, selectedAgentId]);
  const leadAgent = useMemo(() => agents.find((a) => a.isManager) ?? null, [agents]);

  // Onboarding writes the answers onto the lead agent, then drops you into its chat.
  const finishOnboarding = async (patch: OnboardingPatch) => {
    if (leadAgent) {
      await persistAgent({ ...leadAgent, name: patch.name, objective: patch.objective, persona: patch.persona, model: patch.model });
    }
    localStorage.setItem('laos.onboarded', '1');
    setShowOnboarding(false);
    setView('manager');
  };

  const dismissOnboarding = () => {
    localStorage.setItem('laos.onboarded', '1');
    setShowOnboarding(false);
  };

  // "Settings" from an agent's context menu: the lead opens its config panel, a
  // regular agent opens its Config tab. The nonce lets a repeat request re-fire.
  const [configRequest, setConfigRequest] = useState<{ id: string; n: number; tab: 'chat' | 'config' }>({ id: '', n: 0, tab: 'chat' });
  const openAgentSettings = (id: string) => {
    const target = agents.find((a) => a.id === id);
    if (!target) return;
    if (target.isManager) setView('manager');
    else { setSelectedAgentId(id); setView('agent'); }
    setConfigRequest((r) => ({ id, n: r.n + 1, tab: 'config' }));
  };

  const openAgent = (id: string) => {
    // The Manager is a root-level view; opening it goes to the Manager tab.
    if (agents.find((a) => a.id === id)?.isManager) {
      setView('manager');
      return;
    }
    setSelectedAgentId(id);
    setView('agent');
    // Reopening a row always lands on the chat, even if Settings was open.
    setConfigRequest((r) => ({ id, n: r.n + 1, tab: 'chat' }));
  };

  const addAgent = async () => {
    const agent = await createAgent({});
    setSelectedAgentId(agent.id);
    setView('agent');
    toast(`agent "${agent.name}" created`, 'success');
  };

  return (
    <div className="app-drag flex h-screen overflow-hidden">
      {/* Left section: the action rail conjoined with the chat list — one panel,
          one clean border. */}
      <div className="app-no-drag my-3 ml-3 flex min-h-0 shrink-0 overflow-hidden rounded-xl border border-border bg-surface">
        <Rail
          collapsed={sidebarCollapsed}
          onToggleSidebar={() => setSidebarCollapsed((c) => !c)}
          view={view}
          setView={setView}
          onNewAgent={addAgent}
          onOpenGraph={() => setView('graph')}
          graphActive={view === 'graph'}
        />
        <Sidebar
          agents={agents}
          view={view}
          selectedAgentId={selectedAgentId}
          onOpen={openAgent}
          onSettings={openAgentSettings}
          onTogglePin={(id, pinned) => {
            const a = agents.find((x) => x.id === id);
            if (a) void persistAgent({ ...a, pinned });
          }}
          onDelete={async (id) => {
            await deleteAgent(id);
            if (selectedAgentId === id) { setSelectedAgentId(null); setView('home'); }
            toast('agent deleted', 'success');
          }}
          collapsed={sidebarCollapsed}
        />
      </div>

      {/* Right section: free canvas — chrome floats on top, content is full-bleed. */}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <Topbar />
        <main className="app-no-drag min-h-0 min-w-0 flex-1 overflow-hidden">
          {/* Views stay mounted; hidden ones keep their live state (chats, streaming). */}
          <div className={pane(view === 'home')}><HomeView agents={agents} tools={tools} workflows={workflows} runs={runs} onOpen={openAgent} onCreate={addAgent} onOpenWorkflow={(id) => { setView('workflows'); setWorkflowToOpen(id); }} /></div>
          <div className={pane(view === 'graph')}>
            <GraphView
              agents={agents} skills={skills} tools={tools} integrations={integrations} workflows={workflows}
              onOpenAgent={openAgent}
              onOpenWorkflow={(id) => { setView('workflows'); setWorkflowToOpen(id); }}
              onSaveAgent={persistAgent}
              onSaveWorkflow={saveWorkflow}
            />
          </div>
          <div className={pane(view === 'agents')}><AgentsView agents={agents} onOpen={openAgent} onCreate={addAgent} onSettings={openAgentSettings} onTogglePin={(id, pinned) => { const a = agents.find((x) => x.id === id); if (a) void persistAgent({ ...a, pinned }); }} onDelete={async (id) => { await deleteAgent(id); if (selectedAgentId === id) setSelectedAgentId(null); toast('agent deleted', 'success'); }} /></div>
          <div className={pane(view === 'workflows')}>
            <CanvasView
              agents={agents} tools={tools} workflows={workflows} integrations={integrations}
              initialWorkflowId={workflowToOpen}
              onInitialWorkflowConsumed={() => setWorkflowToOpen(null)}
              onSaveWorkflow={saveWorkflow}
              onDeleteWorkflow={deleteWorkflow}
              onRunWorkflow={runWorkflow}
            />
          </div>
          <div className={pane(view === 'manager')}><ManagerView agents={agents} integrations={integrations} models={models} openConfigRequest={configRequest.id === leadAgent?.id ? configRequest.n : 0} /></div>
          <div className={pane(view === 'tasks')}><TasksView agents={agents} /></div>
          <div className={pane(view === 'telegram')}><TelegramView /></div>
          <div className={pane(view === 'runs')}>
            <RunsConsole runs={runs} agents={agents} onOpenAgent={openAgent} onClear={clearRuns} />
          </div>
          <div className={pane(view === 'workshop')}>
            <WorkshopView
              skills={skills} tools={tools} integrations={integrations}
              onAddSkill={() => setDrawerForm({ kind: 'skill', editing: emptySkill(), isNew: true })}
              onEditSkill={(s) => setDrawerForm({ kind: 'skill', editing: s, isNew: false })}
              onDeleteSkill={async (id) => { await deleteSkill(id); setAgents((prev) => prev.map((a) => ({ ...a, skillIds: a.skillIds.filter((s) => s !== id) }))); toast('skill deleted', 'success'); }}
              onAddTool={() => setDrawerForm({ kind: 'tool', editing: emptyTool(), isNew: true })}
              onEditTool={(t) => setDrawerForm({ kind: 'tool', editing: t, isNew: false })}
              onDeleteTool={async (id) => { await deleteTool(id); setTools((prev) => prev.filter((p) => p.id !== id)); setAgents((prev) => prev.map((a) => ({ ...a, toolIds: a.toolIds.filter((t) => t !== id) }))); toast('tool deleted', 'success'); }}
            />
          </div>
          <div className={pane(view === 'settings')}>
            <SettingsView
              models={models}
              onRerunOnboarding={() => setShowOnboarding(true)}
              onAddModel={() => setDrawerForm({ kind: 'model', editing: emptyModel(), isNew: true })}
              onEditModel={(m) => setDrawerForm({ kind: 'model', editing: m, isNew: false })}
              onDeleteModel={deleteModel}
            />
          </div>
          {selectedAgent && (
            <div className={`h-full pb-4 ${view === 'agent' ? '' : 'hidden'}`}><AgentWindow key={selectedAgent.id} agent={selectedAgent} tools={tools} skills={skills} models={models} integrations={integrations} runs={runs} onSave={persistAgent} onDelete={async (id) => { await deleteAgent(id); setSelectedAgentId(null); setView('agents'); }} onRun={handleRun} tabRequest={{ tab: configRequest.tab, n: configRequest.id === selectedAgent.id ? configRequest.n : 0 }} /></div>
          )}
        </main>
      </div>

      {drawerForm && drawerForm.kind === 'tool' && (
        <ToolFormDrawer
          editing={drawerForm.editing} isNew={drawerForm.isNew} integrations={integrations}
          onClose={() => setDrawerForm(null)}
          onSave={async (t) => { await saveTool(t); setDrawerForm(null); toast('tool saved', 'success'); }}
        />
      )}
      {drawerForm && drawerForm.kind === 'model' && (
        <ModelFormDrawer
          editing={drawerForm.editing} isNew={drawerForm.isNew}
          onClose={() => setDrawerForm(null)}
          onSave={async (m) => { await saveModel(m); setDrawerForm(null); toast('model saved', 'success'); }}
        />
      )}
      {drawerForm && drawerForm.kind === 'skill' && (
        <SkillFormDrawer
          editing={drawerForm.editing} isNew={drawerForm.isNew}
          onClose={() => setDrawerForm(null)}
          onSave={async (s) => { await saveSkill(s); setDrawerForm(null); toast('skill saved', 'success'); }}
        />
      )}
      <Toaster />
      {showOnboarding && leadAgent && (
        <Onboarding manager={leadAgent} models={models} onFinish={finishOnboarding} onSkip={dismissOnboarding} />
      )}
      {update && <UpdatePrompt info={update} onDismiss={() => setUpdate(null)} />}
    </div>
  );
}
