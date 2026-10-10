import { Channel, invoke } from '@tauri-apps/api/core';
import type { Agent, Integration, ModelConfig, Project, Run, RunEvent, Script, Skill, Task, Tool, Workflow, WorkflowRunResult } from './types';

// Streams a chat completion, calling onDelta with each token chunk and onConfirm
// with structured confirmation requests from the Manager.
export type ManagerConfirmRequest = { requestId: string; tool: string; args: Record<string, unknown> };
// A live step event: either a discrete phase (kind + text) or a chunk of the
// model's own reasoning, which the caller appends to the growing thought.
export type StreamStep = { kind: string; text: string; append?: boolean };

// A per-turn id so a cancel targets exactly the turn the user is watching.
export function newChatStreamId(): string {
  return `stream-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

// Asks the backend to stop an in-flight turn. The turn returns cleanly, keeping
// whatever it had already produced.
export async function cancelChatStream(streamId: string): Promise<void> {
  try { await invoke('cancel_chat', { streamId }); } catch { /* browser preview: no Tauri backend */ }
}

export async function streamChat(
  agent: Agent, input: string, isManager: boolean,
  onDelta: (d: string) => void,
  onConfirm?: (r: ManagerConfirmRequest) => void,
  sessionId?: string,
  onStep?: (s: StreamStep) => void,
  streamId?: string,
): Promise<void> {
  const channel = new Channel<string>();
  channel.onmessage = (raw) => {
    // Structured events are JSON; token deltas are plain text.
    if (raw.startsWith('{')) {
      try {
        const evt = JSON.parse(raw);
        if (evt.type === 'confirm') {
          // Never let a control event fall through into the message log.
          onConfirm?.({ requestId: evt.requestId, tool: evt.tool, args: evt.args ?? {} });
          return;
        }
        if (evt.type === 'status') {
          // A real step: which phase the turn is in, and what it's doing in it.
          onStep?.({ kind: String(evt.kind ?? 'think'), text: String(evt.text ?? '') });
          return;
        }
        if (evt.type === 'reasoning') {
          // The model's thinking, chunk by chunk.
          onStep?.({ kind: 'think', text: String(evt.text ?? ''), append: true });
          return;
        }
      } catch { /* not a control event — treat as a delta */ }
    }
    onDelta(raw);
  };
  await invoke('stream_chat', { agent, input, isManager, onEvent: channel, sessionId: sessionId ?? null, streamId: streamId ?? null });
}

// Chat session management (bifurcated history).
export async function listChatSessions(agentId: string): Promise<{ id: string; title: string; createdAt: string; updatedAt: string; projectId: string }[]> {
  try { return await invoke('list_chat_sessions', { agentId }); } catch { return []; }
}
export async function getChatSession(sessionId: string): Promise<{ role: string; content: string; time: string }[]> {
  try { return await invoke('get_chat_session', { sessionId }); } catch { return []; }
}
export async function createChatSession(agentId: string, title: string, projectId?: string | null): Promise<{ id: string; title: string; createdAt: string; updatedAt: string; projectId: string }> {
  return await invoke('create_chat_session', { agentId, title, projectId: projectId ?? null });
}
export async function deleteChatSession(sessionId: string): Promise<void> {
  await invoke('delete_chat_session', { sessionId });
}

export async function renameChatSession(sessionId: string, title: string): Promise<void> {
  await invoke('rename_chat_session', { sessionId, title });
}

// Summarizes and closes a session, folding it into the agent's day context.
export async function closeSession(sessionId: string, agentId: string, model: string): Promise<void> {
  try { await invoke('close_session', { sessionId, agentId, model }); } catch { /* best-effort */ }
}

export async function confirmManagerTool(requestId: string, approved: boolean, tool: string, args: Record<string, unknown>): Promise<string | null> {
  try {
    return await invoke<string | null>('confirm_manager_tool', { requestId, approved, tool, args });
  } catch {
    return null;
  }
}

// Telegram tunnel + webhook (one-click expose).
export type TelegramLogEntry = { direction: string; chatId: string; text: string; reply: string; status: string; detail: string; createdAt: string };
export async function listTelegramLogs(): Promise<TelegramLogEntry[]> {
  try { return await invoke<TelegramLogEntry[]>('list_telegram_logs'); } catch { return []; }
}
// Telegram bots (multiple): each is long-polled and routed to an agent.
export type TelegramBot = { id: string; name: string; agentId: string; terminalId: string; allowedUsers: (string | number)[]; enabled: boolean; token: string; webhookRegistered: boolean };
export async function listTelegramBots(): Promise<TelegramBot[]> {
  try { return await invoke<TelegramBot[]>('list_telegram_bots'); } catch { return []; }
}
export async function saveTelegramBot(bot: TelegramBot): Promise<string> {
  return await invoke<string>('save_telegram_bot', { bot });
}
export async function deleteTelegramBot(id: string): Promise<void> {
  await invoke('delete_telegram_bot', { id });
}
// One tunnel, many bots: registers this bot's webhook at <base>/webhook/telegram/<id>.
export async function setBotWebhook(botId: string, publicUrl?: string): Promise<string> {
  return await invoke<string>('telegram_set_bot_webhook', { botId, publicUrl: publicUrl ?? null });
}
export async function clearBotWebhook(botId: string): Promise<void> {
  await invoke('telegram_clear_bot_webhook', { botId });
}
// Ask the user a question over Telegram; with options it shows inline buttons,
// otherwise it waits for the next message. Returns the answer.
export async function telegramAsk(botId: string, chatId: number, question: string, options: string[], timeoutSecs?: number): Promise<string> {
  return await invoke<string>('telegram_ask', { botId, chatId, question, options, timeoutSecs: timeoutSecs ?? null });
}

// Terminal sessions (PTY): run a real CLI and drive it from the panel or a bot.
export type TerminalSession = { id: string; name: string; command: string; alive: boolean; excerpt: string };
export async function terminalStart(name: string, command: string, cwd?: string, cols?: number, rows?: number): Promise<{ id: string; name: string }> {
  return await invoke('terminal_start', { name, command, cwd: cwd ?? null, cols: cols ?? null, rows: rows ?? null });
}
export async function terminalWrite(id: string, data: string): Promise<void> {
  await invoke('terminal_write', { id, data });
}
export async function terminalRead(id: string, from: number): Promise<{ data: number[]; next: number }> {
  return await invoke('terminal_read', { id, from });
}
export async function terminalResize(id: string, cols: number, rows: number): Promise<void> {
  await invoke('terminal_resize', { id, cols, rows });
}
export async function terminalList(): Promise<TerminalSession[]> {
  try { return await invoke<TerminalSession[]>('terminal_list'); } catch { return []; }
}
export async function terminalKill(id: string): Promise<void> {
  await invoke('terminal_kill', { id });
}
export async function terminalAttach(id: string, onOutput: (s: string) => void): Promise<void> {
  const ch = new Channel<string>();
  ch.onmessage = (s) => onOutput(s);
  await invoke('terminal_attach', { id, onOutput: ch });
}
export type WebhookHealth = {
  tunnelUrl: string | null;
  webhookRegistered: boolean;
  receiverListening: boolean;
  liveTunnel: string | null;
  urlMismatch: boolean;
  telegram: { url?: string; pending_update_count?: number; last_error_message?: string; last_error_date?: number; [k: string]: unknown };
};
export async function telegramWebhookHealth(): Promise<WebhookHealth> {
  try { return await invoke<WebhookHealth>('telegram_webhook_health'); } catch { return { tunnelUrl: null, webhookRegistered: false, receiverListening: false, liveTunnel: null, urlMismatch: false, telegram: {} }; }
}
export async function telegramStartTunnel(onProgress?: (s: string) => void): Promise<string> {
  return await invoke<string>('telegram_start_tunnel', { onProgress: progressChannel(onProgress) });
}
export async function telegramRegisterWebhook(onProgress?: (s: string) => void): Promise<string> {
  return await invoke<string>('telegram_register_webhook', { onProgress: progressChannel(onProgress) });
}
export async function telegramRegisterCustomUrl(publicUrl: string, onProgress?: (s: string) => void): Promise<string> {
  return await invoke<string>('telegram_register_custom_url', { publicUrl, onProgress: progressChannel(onProgress) });
}
export async function telegramStopTunnel(): Promise<void> {
  await invoke('telegram_stop_tunnel');
}
export async function telegramTunnelStatus(): Promise<{ tunnelUrl: string | null; webhookRegistered: boolean }> {
  try { return await invoke('telegram_tunnel_status'); } catch { return { tunnelUrl: null, webhookRegistered: false }; }
}

// Wraps a callback into a Tauri Channel for streaming progress events.
function progressChannel(onProgress?: (s: string) => void): Channel<string> {
  const ch = new Channel<string>();
  ch.onmessage = (s) => onProgress?.(s);
  return ch;
}

// Knowledge base (shared, user-writable persistent docs).
export type KnowledgeDoc = { id: string; title: string; content: string; tags: string[]; updatedAt: string };
export async function listKnowledgeDocs(): Promise<KnowledgeDoc[]> {
  try { return await invoke<KnowledgeDoc[]>('list_knowledge_docs'); } catch { return []; }
}
export async function getKnowledgeDoc(id: string): Promise<KnowledgeDoc> {
  return await invoke<KnowledgeDoc>('get_knowledge_doc', { id });
}
export async function saveKnowledgeDoc(doc: KnowledgeDoc): Promise<void> {
  await invoke('save_knowledge_doc', { doc });
}
export async function deleteKnowledgeDoc(id: string): Promise<void> {
  await invoke('delete_knowledge_doc', { id });
}

// Loads a persisted conversation (assistant+user turns) for an agent.
export async function loadConversation(agentId: string): Promise<{ role: 'user' | 'assistant'; content: string }[]> {
  try {
    return await invoke<{ role: 'user' | 'assistant'; content: string }[]>('get_conversation', { agentId });
  } catch {
    return [];
  }
}

export async function listRuns(): Promise<Run[]> {
  try { return await invoke<Run[]>('list_runs'); } catch { return []; }
}

export async function clearAgentMemory(agentId: string): Promise<void> {
  await invoke('clear_agent_memory', { agentId });
}

// Native agent memory (was the fox MCP server). The Memory view reads these.
export async function amListMemories(agentId: string, scope?: string): Promise<unknown> {
  try { return await invoke('am_list_memories', { agentId, scope: scope ?? null }); } catch { return []; }
}
export async function amSearch(agentId: string, query: string, scope?: string, limit?: number): Promise<unknown> {
  try { return await invoke('am_search', { agentId, scope: scope ?? null, query, limit: limit ?? null }); } catch { return []; }
}
export async function amGetSelfModel(agentId: string, scope?: string): Promise<unknown> {
  try { return await invoke('am_get_self_model', { agentId, scope: scope ?? null }); } catch { return []; }
}
export async function amStats(): Promise<unknown> {
  try { return await invoke('am_stats'); } catch { return null; }
}
export async function amListNamespaces(): Promise<unknown> {
  try { return await invoke('am_list_namespaces'); } catch { return []; }
}
export async function amAddMemory(m: { agentId: string; scope: string; kind: string; title: string; content: string; tags?: unknown; importance?: number; confidence?: number }): Promise<string> {
  return await invoke<string>('am_add_memory', { agentId: m.agentId, scope: m.scope, kind: m.kind, title: m.title, content: m.content, tags: m.tags ?? null, confidence: m.confidence ?? null, importance: m.importance ?? null });
}
export async function amSetSelfModel(agentId: string, scope: string | null, key: string, value: string): Promise<void> {
  await invoke('am_set_self_model', { agentId, scope, key, value, confidence: null, importance: null });
}
export async function amUpdateMemory(id: string, patch: Record<string, unknown>): Promise<void> {
  await invoke('am_update_memory', { id, patch });
}
export async function amForgetMemory(id: string): Promise<void> {
  await invoke('am_forget_memory', { id });
}

export async function executeAgent(agent: Agent, input: string, apiKey?: string): Promise<{ output: string; events: RunEvent[]; runId?: string; promptTokens?: number; completionTokens?: number }> {
  try {
    return await invoke<{ output: string; events: RunEvent[]; runId: string; promptTokens: number; completionTokens: number }>('execute_agent', { agent, input, apiKey });
  } catch (error) {
    // Vite preview is deliberately useful too: Ollama accepts local browser requests.
    // If this is a Tauri error, preserve it so desktop runs never silently bypass storage.
    if ('__TAURI_INTERNALS__' in window) throw error;
    const model = agent.model.startsWith('ollama:') ? agent.model.slice('ollama:'.length) : '';
    if (!model) throw new Error('Use the desktop app for OpenRouter runs so the API key stays behind the Tauri runtime.');
    const response = await fetch('http://127.0.0.1:11434/api/generate', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({model, prompt:`You are ${agent.name}. Objective: ${agent.objective}\n\nTask: ${input}\n\nReturn a concise structured result.`, stream:false}) });
    if (!response.ok) throw new Error(`Could not reach Ollama (${response.status}). Start Ollama, then pull ${model}.`);
    const data = await response.json() as { response?: string };
    const now = new Date().toLocaleTimeString();
    return { output:data.response ?? 'Ollama returned no text.', events:[{time:now,type:'thought',title:'Asked local Ollama',detail:model},{time:new Date().toLocaleTimeString(),type:'result',title:'Generated final result'}] };
  }
}

export async function initializeStorage() {
  try { await invoke('initialize_storage'); } catch { /* Browser preview intentionally has no Tauri backend. */ }
}

export async function listModels(): Promise<ModelConfig[]> {
  try {
    return await invoke<ModelConfig[]>('list_model_configs');
  } catch {
    return [];
  }
}

export async function saveModel(config: ModelConfig): Promise<void> {
  await invoke('save_model_config', { config });
}

export async function deleteModel(id: string): Promise<void> {
  await invoke('delete_model_config', { id });
}

export async function listTools(): Promise<Tool[]> {
  try {
    return await invoke<Tool[]>('list_tools');
  } catch {
    return [];
  }
}

export async function saveTool(tool: Tool): Promise<void> {
  await invoke('save_tool', { tool });
}

export async function deleteTool(id: string): Promise<void> {
  await invoke('delete_tool', { id });
}

export async function listSkills(): Promise<Skill[]> {
  try {
    return await invoke<Skill[]>('list_skills');
  } catch {
    return [];
  }
}

export async function saveSkill(skill: Skill): Promise<void> {
  await invoke('save_skill', { skill });
}

export async function deleteSkill(id: string): Promise<void> {
  await invoke('delete_skill', { id });
}

// Scripts (reusable runnable commands / custom apps).
export async function listScripts(): Promise<Script[]> {
  try {
    return await invoke<Script[]>('list_scripts');
  } catch {
    return [];
  }
}

export async function saveScript(script: Script): Promise<string> {
  return await invoke<string>('save_script', { script });
}

export async function deleteScript(id: string): Promise<void> {
  await invoke('delete_script', { id });
}

export async function runScriptNow(id: string, input?: string): Promise<string> {
  return await invoke<string>('run_script_now', { id, input: input ?? null });
}

// Projects (group conversations + scope their memory).
export async function listProjects(): Promise<Project[]> {
  try {
    return await invoke<Project[]>('list_projects');
  } catch {
    return [];
  }
}
export async function saveProject(project: Project): Promise<string> {
  return await invoke<string>('save_project', { project });
}
export async function deleteProject(id: string): Promise<void> {
  await invoke('delete_project', { id });
}

export async function listAgents(): Promise<Agent[]> {
  try {
    return await invoke<Agent[]>('list_agents');
  } catch {
    return [];
  }
}

export async function saveAgent(agent: Agent): Promise<void> {
  await invoke('save_agent', { agent });
}

export async function deleteAgent(id: string): Promise<void> {
  await invoke('delete_agent', { id });
}

export async function listWorkflows(): Promise<Workflow[]> {
  try {
    return await invoke<Workflow[]>('list_workflows');
  } catch {
    return [];
  }
}

export async function saveWorkflow(workflow: Workflow): Promise<void> {
  await invoke('save_workflow', { workflow });
}

export async function deleteWorkflow(id: string): Promise<void> {
  await invoke('delete_workflow', { id });
}

export async function executeWorkflow(workflow: Workflow, input: string): Promise<WorkflowRunResult> {
  return await invoke<WorkflowRunResult>('execute_workflow', { workflow, input });
}

// Stored workflow runs — executions are persisted server-side, optionally per workflow.
export type WorkflowRunRecord = {
  id: string; workflowId: string; workflowName: string; startedAt: string; endedAt?: string | null;
  status: string; input: string; finalOutput?: string | null;
  steps: { nodeId: string; nodeLabel: string; output: string; promptTokens: number; completionTokens: number }[];
  promptTokens: number; completionTokens: number;
};
export async function listWorkflowRuns(workflowId?: string): Promise<WorkflowRunRecord[]> {
  try { return await invoke<WorkflowRunRecord[]>('list_workflow_runs', { workflowId: workflowId ?? null }); } catch { return []; }
}

// MCP connector (stdio). Servers are configured commands; their advertised tools
// can be imported into the tool registry and attached to agents.
export type McpServer = { id: string; name: string; command: string; args: string[]; env: Record<string, string>; enabled: boolean };
export type McpToolInfo = { name: string; description: string; schema: Record<string, unknown> };
export async function listMcpServers(): Promise<McpServer[]> {
  try { return await invoke<McpServer[]>('list_mcp_servers'); } catch { return []; }
}
export async function saveMcpServer(server: McpServer): Promise<string> {
  return await invoke<string>('save_mcp_server', { server });
}
export async function deleteMcpServer(id: string): Promise<void> {
  await invoke('delete_mcp_server', { id });
}
export async function testMcpServer(id: string): Promise<McpToolInfo[]> {
  return await invoke<McpToolInfo[]>('test_mcp_server', { id });
}
export async function importMcpTools(id: string): Promise<number> {
  return await invoke<number>('import_mcp_tools', { id });
}

// A good/bad signal on a reply: the host records it as experience and reinforces
// or dampens the memories that informed the turn. Best-effort.
export async function rateTurn(agentId: string, signal: 'up' | 'down', content?: string): Promise<void> {
  try { await invoke('rate_turn', { agentId, signal, content: content ?? null }); } catch { /* best-effort */ }
}

// Invokes an MCP server's tool directly (no agent in the loop) — used by surfaces
// like the Memory view to read a server's data. Returns parsed JSON when the
// server responded with JSON, otherwise the raw text.
export async function callMcpTool(serverId: string, tool: string, args: Record<string, unknown>): Promise<unknown> {
  const text = await invoke<string>('call_mcp_tool', { serverId, tool, arguments: args });
  try { return JSON.parse(text); } catch { return text; }
}

export async function listIntegrations(): Promise<Integration[]> {
  try {
    return await invoke<Integration[]>('list_integrations');
  } catch {
    return [];
  }
}

export async function saveIntegrationConfig(id: string, config: Record<string, unknown>): Promise<void> {
  await invoke('save_integration_config', { id, config });
}

export async function testIntegration(id: string): Promise<boolean> {
  try { return await invoke<boolean>('test_integration', { id }); } catch { return false; }
}

export async function startOAuth(id: string): Promise<string> {
  return await invoke<string>('start_oauth', { id });
}

export async function connectOAuth(id: string): Promise<string> {
  return await invoke<string>('connect_oauth', { id });
}

export async function completeOAuth(id: string, code: string): Promise<void> {
  await invoke('complete_oauth', { id, code });
}

export async function managerMessage(message: string): Promise<string> {
  return await invoke<string>('manager_message', { message });
}

export async function listTasks(): Promise<Task[]> {
  try { return await invoke<Task[]>('list_all_tasks'); } catch { return []; }
}

export async function runTask(requester: string, assignedAgent: string, input: string, context: string): Promise<Task> {
  return await invoke<Task>('run_task', { requester, assignedAgent, input, context });
}

export async function cancelTask(id: string): Promise<void> {
  await invoke('cancel_task', { id });
}

// In-app updater. The check/install live in Rust; the UI renders its own prompt.
export type UpdateInfo = { version: string; currentVersion: string; notes?: string | null; date?: string | null };
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  return await invoke<UpdateInfo | null>('check_for_update');
}
export async function installUpdate(): Promise<void> {
  await invoke('install_update');
}
export async function restartApp(): Promise<void> {
  await invoke('restart_app');
}
export async function appVersion(): Promise<string> {
  try { return await invoke<string>('app_version'); } catch { return ''; }
}
