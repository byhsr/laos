import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { Agent, CreateKind, DrawerForm, View } from './types';
import { emptyModel, emptyProject, emptyScript, emptySkill, emptyTool } from './types';
import { useAgentsStore } from './hooks/useAgents';
import { useRunsStore } from './hooks/useRuns';
import { useModelsStore } from './hooks/useModels';
import { useToolsStore } from './hooks/useTools';
import { useSkillsStore } from './hooks/useSkills';
import { useScriptsStore } from './hooks/useScripts';
import { useProjectsStore } from './hooks/useProjects';
import { useWorkflowsStore } from './hooks/useWorkflows';
import { useWorkspaceStore } from './hooks/useWorkspace';
import { useIntegrationsStore } from './hooks/useIntegrations';
import { useProvidersStore } from './hooks/useProviders';
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
import { MemoryView } from './components/views/MemoryView';
import { KnowledgeBaseView } from './components/views/KnowledgeBaseView';
import { TerminalView } from './components/views/TerminalView';
import { ItemsView } from './components/views/ItemsView';
import { NotificationsView } from './components/views/NotificationsView';
import { SkillFormDrawer, SkillsTransferModal } from './components/views/SkillsView';
import { ScriptFormDrawer } from './components/views/ScriptsView';
import { ProjectFormDrawer } from './components/views/ProjectsView';
import { Toaster } from './components/ui/Toaster';
import { ErrorBoundary } from './components/ui/ErrorBoundary';
import { ConfirmDialog } from './components/ui/ConfirmDialog';
import { UpdatePrompt } from './components/ui/UpdatePrompt';
import { BrowserPanel } from './components/BrowserPanel';
import { useBrowserStore } from './hooks/useBrowser';
import { toast } from './hooks/useToast';
import { syncBrowserTheme } from './theme';
import { VoiceOverlay } from './components/voice/VoiceOverlay';
import { useActiveChat } from './hooks/useActiveChat';
import { useVoice } from './hooks/useVoice';
import { useNotifications } from './hooks/useNotifications';
import { checkForUpdate, getAppSettings, onBrainNotice, onBrowserEvent, onNotification, onVoiceToggle, type UpdateInfo } from './runtime';

// Every view lives in a pane that scrolls internally — the shell itself never
// scrolls, so there is never a double scrollbar or a page that jumps.
const pane = (active: boolean) =>
  `h-full overflow-x-hidden overflow-y-auto p-4 ${active ? '' : 'hidden'}`;

// Every view mounts at once, so a view that throws during render would blank the
// whole window. Each pane carries its own boundary: the broken view shows its
// error in place and the rest of the app keeps working.
function Pane({ active, label, children }: { active: boolean; label: string; children: ReactNode }) {
  return <div className={pane(active)}><ErrorBoundary label={label}>{children}</ErrorBoundary></div>;
}

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
  const scripts = useScriptsStore((s) => s.scripts);
  const saveScript = useScriptsStore((s) => s.saveScript);
  const deleteScript = useScriptsStore((s) => s.deleteScript);
  const loadScripts = useScriptsStore((s) => s.loadScripts);
  const projects = useProjectsStore((s) => s.projects);
  const activeProjectId = useProjectsStore((s) => s.activeId);
  const saveProject = useProjectsStore((s) => s.saveProject);
  const deleteProject = useProjectsStore((s) => s.deleteProject);
  const loadProjects = useProjectsStore((s) => s.loadProjects);
  const setActiveProject = useProjectsStore((s) => s.setActive);
  const integrations = useIntegrationsStore((s) => s.integrations);
  const loadIntegrations = useIntegrationsStore((s) => s.loadIntegrations);
  const loadProviders = useProvidersStore((s) => s.loadProviders);
  const loadWorkspace = useWorkspaceStore((s) => s.loadWorkspace);
  const workflows = useWorkflowsStore((s) => s.workflows);
  const saveWorkflow = useWorkflowsStore((s) => s.saveWorkflow);
  const deleteWorkflow = useWorkflowsStore((s) => s.deleteWorkflow);
  const runWorkflow = useWorkflowsStore((s) => s.runWorkflow);
  const loadWorkflows = useWorkflowsStore((s) => s.loadWorkflows);
  const loadRuns = useRunsStore((s) => s.loadRuns);

  useEffect(() => {
    loadAgents(); loadModels(); loadTools(); loadSkills(); loadScripts(); loadProjects(); loadWorkflows(); loadIntegrations(); loadRuns(); loadProviders();
    // Reload tools after the storage bootstrap so MCP tools imported on first run
    // (browser/memory) are present in the picker.
    void loadWorkspace().then(() => loadTools());
    // Populate the notification badge at startup (reminders may have fired while
    // the window was on the tray).
    void useNotifications.getState().load();
    // Point the in-app browser at the restored theme's colour scheme.
    syncBrowserTheme();
  }, [loadAgents, loadModels, loadTools, loadSkills, loadScripts, loadProjects, loadWorkspace, loadWorkflows, loadIntegrations, loadRuns, loadProviders]);

  // Native drag-drop is off (see tauri.conf.json) so the chat composer can accept
  // HTML5 drops. That means every other drop in the window has to be swallowed
  // here, or the webview would navigate away to the dropped file.
  useEffect(() => {
    const swallow = (e: DragEvent) => e.preventDefault();
    window.addEventListener('dragover', swallow);
    window.addEventListener('drop', swallow);
    return () => { window.removeEventListener('dragover', swallow); window.removeEventListener('drop', swallow); };
  }, []);

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
  // The notification queue carries reminders, insights and finished work: toast
  // the live one and let the bell badge update. The queue itself is durable, so a
  // reminder that fired while the window was closed is still in the inbox.
  useEffect(() => onNotification((n) => {
    toast(`${n.title}${n.body ? `: ${n.body}` : ''}`, 'info');
    useNotifications.getState().receive(n);
  }), []);

  // The learning layer announces a pattern it noticed, so it reaches the user
  // where they are rather than waiting for the Memory view to be opened.
  useEffect(() => onBrainNotice((n) => toast(`${n.title} (${n.count})`, 'info')), []);

  // The global hotkey toggles live voice mode; the system voices are the default
  // until Settings says otherwise.
  useEffect(() => onVoiceToggle(() => useVoice.getState().toggle()), []);
  useEffect(() => {
    void getAppSettings().then((s) => useVoice.getState().setTtsMode(s.voiceTtsMode as 'auto' | 'system' | 'provider'));
  }, []);

  // The mini browser reports its state from the backend; a browser action from an
  // agent slides the drawer open (see useBrowserStore.applyState).
  const browserPanelOpen = useBrowserStore((s) => s.panelOpen);
  useEffect(() => onBrowserEvent((s) => useBrowserStore.getState().applyState(s)), []);
  const toggleBrowser = () => {
    const s = useBrowserStore.getState();
    if (s.panelOpen) s.closePanel(); else s.openPanel();
  };

  // Navigation is a history, so the chrome's back/forward buttons are real.
  // The stack holds views only — the selected agent persists on its own, so
  // stepping away from an agent window and back lands on the same agent.
  const [nav, setNav] = useState<{ stack: View[]; i: number }>({ stack: ['home'], i: 0 });
  const view = nav.stack[nav.i];
  const setView = (v: View) => setNav((n) => (n.stack[n.i] === v ? n : { stack: [...n.stack.slice(0, n.i + 1), v], i: n.i + 1 }));
  const canBack = nav.i > 0;
  const canForward = nav.i < nav.stack.length - 1;
  const goBack = () => setNav((n) => (n.i > 0 ? { ...n, i: n.i - 1 } : n));
  const goForward = () => setNav((n) => (n.i < n.stack.length - 1 ? { ...n, i: n.i + 1 } : n));
  const [drawerForm, setDrawerForm] = useState<DrawerForm>(null);
  // The skills import/export modal (Workshop → skills). Separate from `drawerForm`
  // because it is not a create/edit drawer.
  const [skillTransfer, setSkillTransfer] = useState<'import' | 'export' | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [workflowToOpen, setWorkflowToOpen] = useState<string | null>(null);
  const building = view === 'builder';

  // The canvas and the in-app browser are both wide, so the chat list tucks away
  // whenever either is open and comes back exactly as it was on the way out.
  const autoCollapseSidebar = building || browserPanelOpen;
  const collapsedBeforeAuto = useRef<boolean | null>(null);
  useEffect(() => {
    if (autoCollapseSidebar) {
      if (collapsedBeforeAuto.current === null) collapsedBeforeAuto.current = sidebarCollapsed;
      setSidebarCollapsed(true);
    } else if (collapsedBeforeAuto.current !== null) {
      setSidebarCollapsed(collapsedBeforeAuto.current);
      collapsedBeforeAuto.current = null;
    }
    // Only the entry/exit matters — a manual toggle while it's open stands.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoCollapseSidebar]);

  const selectedAgent = useMemo(() => agents.find((a) => a.id === selectedAgentId) ?? null, [agents, selectedAgentId]);
  const leadAgent = useMemo(() => agents.find((a) => a.isManager) ?? null, [agents]);

  // Remember which chat is in front of the user so the voice hotkey talks to it
  // (and falls back to the lead agent when no chat is open).
  useEffect(() => {
    if (view === 'manager') useActiveChat.getState().setActiveAgent(leadAgent?.id ?? null);
    else if (view === 'agent' && selectedAgentId) useActiveChat.getState().setActiveAgent(selectedAgentId);
  }, [view, selectedAgentId, leadAgent?.id]);

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

  // One entry point for the "+" button: left-click runs the persisted default,
  // the right-click menu runs any of them. Each branch reuses the exact path the
  // entity's own view already uses.
  const createEntity = (kind: CreateKind) => {
    switch (kind) {
      case 'agent': void addAgent(); break;
      case 'workflow': setWorkflowToOpen('new'); setView('builder'); break;
      case 'skill': setDrawerForm({ kind: 'skill', editing: emptySkill(), isNew: true }); break;
      case 'script': setDrawerForm({ kind: 'script', editing: emptyScript(), isNew: true }); break;
      case 'project': setDrawerForm({ kind: 'project', editing: emptyProject(), isNew: true }); break;
      case 'tool': setDrawerForm({ kind: 'tool', editing: emptyTool(), isNew: true }); break;
      case 'model': setDrawerForm({ kind: 'model', editing: emptyModel(), isNew: true }); break;
    }
  };

  return (
    // The window drags from the Topbar alone — never from the whole shell. The
    // shell used to carry app-drag with no-drag holes, but WebView2 v152 made
    // app-region an inherited property, so glass's full-bleed ::before/::after
    // frost layers inherited `drag` and covered every click as a window drag.
    <div className="app-shell relative flex h-screen flex-col overflow-hidden">
      <Topbar
        collapsed={sidebarCollapsed}
        onToggleSidebar={() => setSidebarCollapsed((c) => !c)}
        canBack={canBack}
        canForward={canForward}
        onBack={goBack}
        onForward={goForward}
        onOpenGraph={() => setView('graph')}
        graphActive={view === 'graph'}
        onCreate={createEntity}
        onToggleBrowser={toggleBrowser}
        browserActive={browserPanelOpen}
      />

      <div className="flex min-h-0 flex-1">
        {/* The mini browser is a real column in the layout, so the chat stays
            visible beside it (it never overlays). Its webview is hidden while a
            modal/drawer is up so it can't paint over it. */}
        {browserPanelOpen && (
          <BrowserPanel hidden={!!drawerForm || showOnboarding || !!update} />
        )}
        {/* Left section: the action rail conjoined with the chat list — one
            panel, one clean border. */}
        <div className="app-no-drag my-3 ml-3 flex min-h-0 shrink-0 overflow-hidden rounded-xl border border-border bg-surface">
          <Rail view={view === 'builder' ? 'workflows' : view} setView={setView} />
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

        {/* Right section: free canvas — content is full-bleed */}
        <main className="app-no-drag min-h-0 min-w-0 flex-1 overflow-hidden">
          {/* Views stay mounted; hidden ones keep their live state (chats, streaming). */}
          <Pane active={view === 'home'} label="home"><HomeView agents={agents} tools={tools} workflows={workflows} runs={runs} onOpen={openAgent} onCreate={addAgent} onOpenWorkflow={(id) => { setView('builder'); setWorkflowToOpen(id); }} /></Pane>
          <Pane active={view === 'graph'} label="graph">
            <GraphView
              agents={agents} skills={skills} tools={tools} integrations={integrations} workflows={workflows} projects={projects}
              onOpenAgent={openAgent}
              onOpenWorkflow={(id) => { setView('builder'); setWorkflowToOpen(id); }}
              onSaveAgent={persistAgent}
              onSaveWorkflow={saveWorkflow}
            />
          </Pane>
          <Pane active={view === 'agents'} label="agents"><AgentsView agents={agents} onOpen={openAgent} onCreate={addAgent} onSettings={openAgentSettings} onTogglePin={(id, pinned) => { const a = agents.find((x) => x.id === id); if (a) void persistAgent({ ...a, pinned }); }} onDelete={async (id) => { await deleteAgent(id); if (selectedAgentId === id) setSelectedAgentId(null); toast('agent deleted', 'success'); }} /></Pane>
          <Pane active={view === 'workflows' || view === 'builder'} label="workflows">
            <CanvasView
              agents={agents} tools={tools} workflows={workflows} integrations={integrations} models={models} scripts={scripts}
              initialWorkflowId={workflowToOpen}
              onInitialWorkflowConsumed={() => setWorkflowToOpen(null)}
              building={building}
              onOpen={() => setView('builder')}
              onSaveWorkflow={saveWorkflow}
              onDeleteWorkflow={deleteWorkflow}
              onRunWorkflow={runWorkflow}
            />
          </Pane>
          <Pane active={view === 'manager'} label="manager"><ManagerView agents={agents} integrations={integrations} models={models} tools={tools} openConfigRequest={configRequest.id === leadAgent?.id ? configRequest.n : 0} /></Pane>
          <Pane active={view === 'memory'} label="memory"><MemoryView /></Pane>
          <Pane active={view === 'knowledge'} label="knowledge"><KnowledgeBaseView /></Pane>
          <Pane active={view === 'tasks'} label="tasks"><TasksView agents={agents} /></Pane>
          <Pane active={view === 'telegram'} label="telegram"><TelegramView /></Pane>
          <Pane active={view === 'terminal'} label="terminal"><TerminalView /></Pane>
          <Pane active={view === 'items'} label="items"><ItemsView /></Pane>
          <Pane active={view === 'notifications'} label="notifications"><NotificationsView /></Pane>
          <Pane active={view === 'runs'} label="runs">
            <RunsConsole runs={runs} agents={agents} onOpenAgent={openAgent} onClear={clearRuns} />
          </Pane>
          <Pane active={view === 'workshop'} label="workshop">
            <WorkshopView
              skills={skills} scripts={scripts} projects={projects} activeProjectId={activeProjectId} tools={tools} integrations={integrations}
              onAddSkill={() => setDrawerForm({ kind: 'skill', editing: emptySkill(), isNew: true })}
              onEditSkill={(s) => setDrawerForm({ kind: 'skill', editing: s, isNew: false })}
              onDeleteSkill={async (id) => { await deleteSkill(id); setAgents((prev) => prev.map((a) => ({ ...a, skillIds: (a.skillIds ?? []).filter((s) => s !== id) }))); toast('skill deleted', 'success'); }}
              onImportSkill={() => setSkillTransfer('import')}
              onExportSkill={() => setSkillTransfer('export')}
              onAddScript={() => setDrawerForm({ kind: 'script', editing: emptyScript(), isNew: true })}
              onEditScript={(s) => setDrawerForm({ kind: 'script', editing: s, isNew: false })}
              onDeleteScript={async (id) => { await deleteScript(id); toast('script deleted', 'success'); }}
              onAddProject={() => setDrawerForm({ kind: 'project', editing: emptyProject(), isNew: true })}
              onEditProject={(p) => setDrawerForm({ kind: 'project', editing: p, isNew: false })}
              onDeleteProject={async (id) => { await deleteProject(id); toast('project deleted', 'success'); }}
              onSetActiveProject={setActiveProject}
              onAddTool={() => setDrawerForm({ kind: 'tool', editing: emptyTool(), isNew: true })}
              onEditTool={(t) => setDrawerForm({ kind: 'tool', editing: t, isNew: false })}
              onDeleteTool={async (id) => { await deleteTool(id); setTools((prev) => prev.filter((p) => p.id !== id)); setAgents((prev) => prev.map((a) => ({ ...a, toolIds: (a.toolIds ?? []).filter((t) => t !== id) }))); toast('tool deleted', 'success'); }}
            />
          </Pane>
          <Pane active={view === 'settings'} label="settings">
            <SettingsView
              models={models}
              onRerunOnboarding={() => setShowOnboarding(true)}
              onAddModel={() => setDrawerForm({ kind: 'model', editing: emptyModel(), isNew: true })}
              onEditModel={(m) => setDrawerForm({ kind: 'model', editing: m, isNew: false })}
              onDeleteModel={deleteModel}
              onNavigate={setView}
            />
          </Pane>
          {selectedAgent && (
            <div className={`h-full pb-4 ${view === 'agent' ? '' : 'hidden'}`}><ErrorBoundary label="agent"><AgentWindow key={selectedAgent.id} agent={selectedAgent} tools={tools} skills={skills} models={models} integrations={integrations} runs={runs} onSave={persistAgent} onDelete={async (id) => { await deleteAgent(id); setSelectedAgentId(null); setView('agents'); }} onRun={handleRun} tabRequest={{ tab: configRequest.tab, n: configRequest.id === selectedAgent.id ? configRequest.n : 0 }} /></ErrorBoundary></div>
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
          editing={drawerForm.editing} isNew={drawerForm.isNew} scripts={scripts}
          onClose={() => setDrawerForm(null)}
          onSave={async (s) => { await saveSkill(s); setDrawerForm(null); toast('skill saved', 'success'); }}
        />
      )}
      {skillTransfer && (
        <SkillsTransferModal
          mode={skillTransfer}
          onClose={() => setSkillTransfer(null)}
          onImported={loadSkills}
        />
      )}
      {drawerForm && drawerForm.kind === 'script' && (
        <ScriptFormDrawer
          editing={drawerForm.editing} isNew={drawerForm.isNew} models={models}
          onClose={() => setDrawerForm(null)}
          onSave={async (s) => { await saveScript(s); setDrawerForm(null); toast('app saved', 'success'); }}
        />
      )}
      {drawerForm && drawerForm.kind === 'project' && (
        <ProjectFormDrawer
          editing={drawerForm.editing} isNew={drawerForm.isNew}
          onClose={() => setDrawerForm(null)}
          onSave={async (p) => { await saveProject(p); setDrawerForm(null); toast('project saved', 'success'); }}
        />
      )}
      {/* Tool approval is a modal at the root, so it works from any view (agent
          or Manager chat) rather than only while the composer is mounted. */}
      <ConfirmDialog />
      <VoiceOverlay onOpenSettings={() => setView('settings')} />
      <Toaster />
      {showOnboarding && leadAgent && (
        <Onboarding manager={leadAgent} models={models} onFinish={finishOnboarding} onSkip={dismissOnboarding} />
      )}
      {update && <UpdatePrompt info={update} onDismiss={() => setUpdate(null)} />}
    </div>
  );
}
