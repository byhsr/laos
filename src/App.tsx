import { useEffect, useMemo, useState } from 'react';
import type { Agent, DrawerForm, View } from './types';
import { emptyModel, emptyTool } from './types';
import { useAgentsStore } from './hooks/useAgents';
import { useRunsStore } from './hooks/useRuns';
import { useModelsStore } from './hooks/useModels';
import { useToolsStore } from './hooks/useTools';
import { useWorkflowsStore } from './hooks/useWorkflows';
import { useWorkspaceStore } from './hooks/useWorkspace';
import { useIntegrationsStore } from './hooks/useIntegrations';
import { Topbar } from './components/Topbar';
import { Sidebar } from './components/Sidebar';
import { AgentWindow } from './components/AgentWindow';
import { HomeView } from './components/views/HomeView';
import { AgentsView } from './components/views/AgentsView';
import { CanvasView } from './components/views/CanvasView';
import { RunsConsole } from './components/views/RunsConsole';
import { IntegrationsView } from './components/views/IntegrationsView';
import { ToolsView, ToolFormDrawer } from './components/views/ToolsView';
import { ModelsView, ModelFormDrawer } from './components/views/ModelsView';
import { SettingsView } from './components/views/SettingsView';
import { ManagerView } from './components/views/ManagerView';
import { TasksView } from './components/views/TasksView';
import { Toaster } from './components/ui/Toaster';
import { toast } from './hooks/useToast';
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
    loadAgents(); loadModels(); loadTools(); loadWorkspace(); loadWorkflows(); loadIntegrations(); loadRuns();
  }, [loadAgents, loadModels, loadTools, loadWorkspace, loadWorkflows, loadIntegrations, loadRuns]);

  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [view, setView] = useState<View>('home');
  const [drawerForm, setDrawerForm] = useState<DrawerForm>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);

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
      <Topbar collapsed={sidebarCollapsed} onToggleSidebar={() => setSidebarCollapsed((c) => !c)} />
      <div className="relative flex min-h-0 flex-1">
        <Sidebar view={view} setView={setView} collapsed={sidebarCollapsed} />
        <main className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto bg-bg px-9 py-8">
          {view === 'home' && <HomeView agents={agents} tools={tools} runs={runs} onOpen={openAgent} onCreate={addAgent} />}
          {view === 'agents' && <AgentsView agents={agents} tools={tools} onOpen={openAgent} onCreate={addAgent} onDelete={async (id) => { await deleteAgent(id); if (selectedAgentId === id) setSelectedAgentId(null); toast('Agent deleted', 'success'); }} />}
          {view === 'workflows' && (
            <CanvasView
              agents={agents} tools={tools} workflows={workflows}
              onSaveWorkflow={saveWorkflow}
              onDeleteWorkflow={deleteWorkflow}
              onRunWorkflow={runWorkflow}
            />
          )}
          {view === 'manager' && <ManagerView agents={agents} integrations={integrations} models={models} />}
          {view === 'tasks' && <TasksView agents={agents} />}
          {view === 'runs' && <RunsConsole runs={runs} agents={agents} onOpenAgent={openAgent} onClear={clearRuns} />}
          {view === 'integrations' && <IntegrationsView integrations={integrations} />}
          {view === 'tools' && <ToolsView tools={tools} integrations={integrations} onAdd={() => setDrawerForm({ kind: 'tool', editing: emptyTool(), isNew: true })} onEdit={(t) => setDrawerForm({ kind: 'tool', editing: t, isNew: false })} onDelete={async (id) => { await deleteTool(id); setTools((prev) => prev.filter((p) => p.id !== id)); setAgents((prev) => prev.map((a) => ({ ...a, toolIds: a.toolIds.filter((t) => t !== id) }))); toast('Tool deleted', 'success'); }} />}
          {view === 'models' && <ModelsView models={models} onAdd={() => setDrawerForm({ kind: 'model', editing: emptyModel(), isNew: true })} onEdit={(m) => setDrawerForm({ kind: 'model', editing: m, isNew: false })} onDelete={deleteModel} />}
          {view === 'settings' && <SettingsView />}
          {view === 'agent' && selectedAgent && <AgentWindow agent={selectedAgent} tools={tools} models={models} integrations={integrations} runs={runs} onBack={() => setView('agents')} onSave={persistAgent} onDelete={async (id) => { await deleteAgent(id); setSelectedAgentId(null); setView('agents'); }} onRun={handleRun} />}
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
      </div>
      <Toaster />
    </div>
  );
}
