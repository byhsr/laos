import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { X } from 'lucide-react';
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
import { UsageView } from './components/views/UsageView';
import { Onboarding, type OnboardingPatch } from './components/Onboarding';
import { WorkshopView } from './components/views/WorkshopView';
import { MemoryView } from './components/views/MemoryView';
import { ConnectionsView } from './components/views/ConnectionsView';
import { TerminalView } from './components/views/TerminalView';
import { ItemsView } from './components/views/ItemsView';
import { NotificationsView } from './components/views/NotificationsView';
import { SkillFormDrawer, SkillsTransferModal } from './components/views/SkillsView';
import { ScriptFormDrawer } from './components/views/ScriptsView';
import { ProjectFormDrawer } from './components/views/ProjectsView';
import { Toaster } from './components/ui/Toaster';
import { ErrorBoundary } from './components/ui/ErrorBoundary';
import { GHOST_BTN } from './components/ui/Button';
import { AgentAvatar } from './components/ui/AgentAvatar';
import { ConfirmDialog } from './components/ui/ConfirmDialog';
import { UpdatePrompt } from './components/ui/UpdatePrompt';
import { BrowserPanel } from './components/BrowserPanel';
import { useBrowserStore } from './hooks/useBrowser';
import { toast } from './hooks/useToast';
import { syncBrowserTheme } from './theme';
import { VoiceOverlay } from './components/voice/VoiceOverlay';
import { useActiveChat } from './hooks/useActiveChat';
import { useManagerStore } from './hooks/useManager';
import { useActivity } from './hooks/useActivity';
import { useVoice } from './hooks/useVoice';
import { useNotifications } from './hooks/useNotifications';
import { checkForUpdate, getAppSettings, onAgentsChanged, onBrainNotice, onBrowserEvent, onNotification, onTaskFinished, onTaskStarted, onVoiceToggle, type UpdateInfo } from './runtime';

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

// At most three agents share the window at once.
const MAX_PANES = 3;

// A split needs its own identity strip: which agent this column is, whether it is
// the focused one, and how to close it. Only rendered once more than one window is
// open — a single agent is the ordinary view and needs no chrome restating it.
// Focus reads as brightness, never as color; the border is the separator.
function PaneBar({ agent, focused, onFocus, onClose }: { agent: Agent; focused: boolean; onFocus: () => void; onClose: () => void }) {
  return (
    <header onMouseDown={onFocus} className={`flex shrink-0 items-center gap-2 border-b border-border bg-surface px-2 py-1 ${focused ? '' : 'opacity-60'}`}>
      <AgentAvatar agent={agent} size={14} />
      <span className={`min-w-0 flex-1 truncate font-mono text-[11px] ${focused ? 'text-foreground' : 'text-muted'}`}>{agent.name}</span>
      <button type="button" className={GHOST_BTN} onClick={onClose} aria-label={`close ${agent.name}`}><X size={12} /></button>
    </header>
  );
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

  // Which agent window is in front. The sidebar's selection, the voice target and
  // every agent link follow this, so nothing else in the app needs to know that a
  // split can exist — a split is simply more than one pane in `paneIds`.
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  // The agent windows open side by side, in order.
  const [paneIds, setPaneIds] = useState<string[]>([]);

  // Show an agent: focus it if its window is already open, otherwise add one. A
  // fourth agent takes over the focused window rather than widening the row, which
  // is what split views do everywhere and keeps the cap of three discoverable.
  function showAgent(id: string) {
    setPaneIds((prev) => {
      if (prev.includes(id)) return prev;
      if (prev.length < MAX_PANES) return [...prev, id];
      const at = selectedAgentId ? prev.indexOf(selectedAgentId) : 0;
      const next = [...prev];
      next[at < 0 ? 0 : at] = id;
      return next;
    });
    setSelectedAgentId(id);
  }

  // A window opened by work rather than by the user. It only takes a free slot: this
  // arrives mid-turn, and replacing or de-focusing whatever the user is looking at
  // would be hostile. Focus is taken only if the row was empty, so the panes are
  // never left without one.
  function showWork(id: string) {
    setPaneIds((prev) => (prev.includes(id) || prev.length >= MAX_PANES ? prev : [...prev, id]));
    setSelectedAgentId((cur) => cur ?? id);
  }

  // Closing hands focus to the window that slid into its place, so the row is never
  // left without one.
  function closePane(id: string) {
    const at = paneIds.indexOf(id);
    const next = paneIds.filter((x) => x !== id);
    setPaneIds(next);
    if (id === selectedAgentId) setSelectedAgentId(next[Math.min(at, next.length - 1)] ?? null);
  }

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

  // A background job shows in the chat's activity panel the moment it starts, and a
  // job the Manager handed to another agent also opens that agent's window — work
  // farmed out is watchable while it runs instead of only reported when it lands.
  // The view is deliberately left alone: being pulled out of the Manager's chat
  // mid-turn would be worse than having to glance at the row.
  useEffect(() => onTaskStarted((t) => {
    useActivity.getState().startTask(t);
    useActivity.getState().openPanel();
    if (!t.assignedAgent || t.assignedAgent === t.chatAgentId) return;
    // The agent may have been created earlier in this same turn, so it can still be
    // missing from the roster.
    if (!agents.some((a) => a.id === t.assignedAgent)) void loadAgents();
    showWork(t.assignedAgent);
  }), [agents]);

  // The Manager creates and deletes agents from a chat turn, and the roster is only
  // read at startup — without this its change stays invisible until a reload. A new
  // agent also gets a window, so one the Manager spun up can be watched and steered.
  useEffect(() => onAgentsChanged((a) => {
    void loadAgents();
    if (!a.deleted) showWork(a.id);
  }), [loadAgents]);

  // A background task finishing marks its card (and fires the burst); the agent
  // then writes the follow-up summary itself, streaming into the chat. One
  // follow-up per task, however many times the completion event arrives.
  const handledTasks = useRef<Set<string>>(new Set());
  useEffect(() => onTaskFinished((t) => {
    useActivity.getState().finishTask(t);
    if (t.status === 'cancelled') return;
    if (handledTasks.current.has(t.taskId)) return;
    handledTasks.current.add(t.taskId);
    const agent = agents.find((a) => a.id === t.chatAgentId) ?? agents.find((a) => a.isManager);
    if (agent) void useManagerStore.getState().takeTaskResult(agent, t.sessionId, t.result);
  }), [agents]);

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

  // Resolve the open windows against the roster, so an agent deleted from a chat
  // turn drops its pane instead of leaving an empty column behind.
  const panes = useMemo(() => paneIds.map((id) => agents.find((a) => a.id === id)).filter((a): a is Agent => !!a), [paneIds, agents]);
  const leadAgent = useMemo(() => agents.find((a) => a.isManager) ?? null, [agents]);

  // If the focused agent disappears, focus what is left of the row.
  useEffect(() => {
    if (panes.length && !panes.some((p) => p.id === selectedAgentId)) setSelectedAgentId(panes[0].id);
  }, [panes, selectedAgentId]);

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
    else { showAgent(id); setView('agent'); }
    setConfigRequest((r) => ({ id, n: r.n + 1, tab: 'config' }));
  };

  const openAgent = (id: string) => {
    // The Manager is a root-level view; opening it goes to the Manager tab.
    if (agents.find((a) => a.id === id)?.isManager) {
      setView('manager');
      return;
    }
    showAgent(id);
    setView('agent');
    // Reopening a row always lands on the chat, even if Settings was open.
    setConfigRequest((r) => ({ id, n: r.n + 1, tab: 'chat' }));
  };

  const addAgent = async () => {
    const agent = await createAgent({});
    showAgent(agent.id);
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
          <Pane active={view === 'home'} label="home"><HomeView agents={agents} tools={tools} workflows={workflows} runs={runs} onOpen={openAgent} onCreate={addAgent} onOpenWorkflow={(id) => { setView('builder'); setWorkflowToOpen(id); }} onOpenUsage={() => setView('usage')} /></Pane>
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
          <Pane active={view === 'connections'} label="connections"><ConnectionsView integrations={integrations} /></Pane>
          <Pane active={view === 'tasks'} label="tasks"><TasksView agents={agents} /></Pane>
          <Pane active={view === 'telegram'} label="telegram"><TelegramView /></Pane>
          <Pane active={view === 'terminal'} label="terminal"><TerminalView /></Pane>
          <Pane active={view === 'items'} label="items"><ItemsView /></Pane>
          <Pane active={view === 'notifications'} label="notifications"><NotificationsView /></Pane>
          <Pane active={view === 'runs'} label="runs">
            <RunsConsole runs={runs} agents={agents} onOpenAgent={openAgent} onClear={clearRuns} />
          </Pane>
          <Pane active={view === 'usage'} label="usage">
            <UsageView runs={runs} agents={agents} onOpenAgent={openAgent} />
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
          {/* Agent windows, one to three, side by side. A single window is the
              ordinary agent view and looks exactly as it always has; the identity
              strip only appears once the row is wider than one. */}
          <div className={`h-full min-h-0 pb-4 ${view === 'agent' ? 'flex flex-row' : 'hidden'}`}>
            {panes.map((a, i) => (
              <div key={a.id} className={`flex h-full min-h-0 min-w-0 flex-1 flex-col ${i > 0 ? 'border-l border-border' : ''}`}>
                {panes.length > 1 && <PaneBar agent={a} focused={a.id === selectedAgentId} onFocus={() => setSelectedAgentId(a.id)} onClose={() => closePane(a.id)} />}
                <div className="min-h-0 flex-1">
                  <ErrorBoundary label="agent">
                    <AgentWindow key={a.id} agent={a} tools={tools} skills={skills} models={models} integrations={integrations} runs={runs} onSave={persistAgent} docked={a.id === selectedAgentId} onDelete={async (id) => { await deleteAgent(id); closePane(id); if (paneIds.filter((p) => p !== id).length === 0) setView('agents'); }} onRun={handleRun} onNavigate={setView} tabRequest={{ tab: configRequest.tab, n: configRequest.id === a.id ? configRequest.n : 0 }} />
                  </ErrorBoundary>
                </div>
              </div>
            ))}
          </div>
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
