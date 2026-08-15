import { useMemo, useState } from 'react';
import type { Agent, DrawerForm, View } from './types';
import { emptyModel, emptyTool } from './types';
import { useAgents } from './hooks/useAgents';
import { useRuns } from './hooks/useRuns';
import { useModels } from './hooks/useModels';
import { useTools } from './hooks/useTools';
import { useWorkspace } from './hooks/useWorkspace';
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
import { Plus } from 'lucide-react';

export default function App() {
  const { agents, setAgents, persistAgent, deleteAgent, createAgent } = useAgents();
  const { runs, handleRun, clearRuns } = useRuns();
  const { models, saveModel, deleteModel } = useModels();
  const { tools, setTools, saveTool, deleteTool } = useTools();
  const { integrations } = useWorkspace();

  const [edges] = useState<{ id: string; from: string; to: string }[]>([]);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [view, setView] = useState<View>('home');
  const [drawerForm, setDrawerForm] = useState<DrawerForm>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);

  const selectedAgent = useMemo(() => agents.find((a) => a.id === selectedAgentId) ?? null, [agents, selectedAgentId]);

  const openAgent = (id: string) => {
    setSelectedAgentId(id);
    setView('agent');
  };

  const addAgent = async () => {
    const agent = await createAgent({});
    setSelectedAgentId(agent.id);
    setView('agent');
  };

  return (
    <div className="relative flex h-screen flex-col overflow-hidden">
      <Topbar collapsed={sidebarCollapsed} onToggleSidebar={() => setSidebarCollapsed((c) => !c)} />
      <div className="flex min-h-0 flex-1">
        <Sidebar view={view} setView={setView} collapsed={sidebarCollapsed} />
        <main className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto bg-bg px-9 py-8">
          {view === 'home' && <HomeView agents={agents} tools={tools} runs={runs} onOpen={openAgent} onCreate={addAgent} />}
          {view === 'agents' && <AgentsView agents={agents} tools={tools} onOpen={openAgent} onCreate={addAgent} onDelete={async (id) => { await deleteAgent(id); if (selectedAgentId === id) setSelectedAgentId(null); }} />}
          {view === 'canvas' && (
            <CanvasView
              agents={agents} edges={edges} tools={tools} selectedAgentId={selectedAgentId}
              onSelect={(id) => setSelectedAgentId(id)}
              onOpen={() => { if (selectedAgentId) openAgent(selectedAgentId); }}
              onCreate={addAgent}
            />
          )}
          {view === 'runs' && <RunsConsole runs={runs} agents={agents} onOpenAgent={openAgent} onClear={clearRuns} />}
          {view === 'integrations' && <IntegrationsView integrations={integrations} />}
          {view === 'tools' && <ToolsView tools={tools} integrations={integrations} onAdd={() => setDrawerForm({ kind: 'tool', editing: emptyTool(), isNew: true })} onEdit={(t) => setDrawerForm({ kind: 'tool', editing: t, isNew: false })} onDelete={async (id) => { await deleteTool(id); setTools((prev) => prev.filter((p) => p.id !== id)); setAgents((prev) => prev.map((a) => ({ ...a, toolIds: a.toolIds.filter((t) => t !== id) }))); }} />}
          {view === 'models' && <ModelsView models={models} onAdd={() => setDrawerForm({ kind: 'model', editing: emptyModel(), isNew: true })} onEdit={(m) => setDrawerForm({ kind: 'model', editing: m, isNew: false })} onDelete={deleteModel} />}
          {view === 'settings' && <SettingsView />}
          {view === 'agent' && selectedAgent && <AgentWindow agent={selectedAgent} tools={tools} models={models} integrations={integrations} runs={runs} onBack={() => setView('agents')} onSave={persistAgent} onDelete={async (id) => { await deleteAgent(id); setSelectedAgentId(null); setView('agents'); }} onRun={handleRun} />}
        </main>
      </div>
      {drawerForm && drawerForm.kind === 'tool' && (
        <ToolFormDrawer
          editing={drawerForm.editing} isNew={drawerForm.isNew} integrations={integrations}
          onClose={() => setDrawerForm(null)}
          onSave={async (t) => { await saveTool(t); setDrawerForm(null); }}
        />
      )}
      {drawerForm && drawerForm.kind === 'model' && (
        <ModelFormDrawer
          editing={drawerForm.editing} isNew={drawerForm.isNew}
          onClose={() => setDrawerForm(null)}
          onSave={async (m) => { await saveModel(m); setDrawerForm(null); }}
        />
      )}
    </div>
  );
}
