import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Agent, Attachment, BrainGraph, BrainObservation, Integration, ModelConfig, ModelRoles, Project, ProviderRecord, Run, RunEvent, Script, Skill, Task, Tool, Workflow, WorkflowRunResult } from './types';

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
  attachments?: Attachment[],
  system?: boolean,
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
  await invoke('stream_chat', { agent, input, isManager, onEvent: channel, sessionId: sessionId ?? null, streamId: streamId ?? null, attachments: attachments?.length ? attachments : null, system: system === true });
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

// The agent's own working pad for a chat — read-only here, the agent owns it.
// Purely a peek: nothing in it is the user's to keep.
export async function getScratchpad(sessionId: string | null, agentId: string): Promise<string> {
  try { return await invoke('get_scratchpad', { sessionId: sessionId ?? null, agentId }); } catch { return ''; }
}

export async function confirmManagerTool(requestId: string, approved: boolean, tool: string, args: Record<string, unknown>): Promise<string | null> {
  try {
    return await invoke<string | null>('confirm_manager_tool', { requestId, approved, tool, args });
  } catch {
    return null;
  }
}

// Clears a remembered "blocked" microphone permission in the webview, so the
// prompt returns. No-op on platforms without the API (and in browser preview).
export async function resetMicrophonePermission(): Promise<void> {
  try { await invoke('reset_microphone_permission'); } catch { /* unsupported */ }
}

// Telegram tunnel + webhook (one-click expose).
export type TelegramLogEntry = { direction: string; chatId: string; text: string; reply: string; status: string; detail: string; createdAt: string };
export async function listTelegramLogs(): Promise<TelegramLogEntry[]> {
  try { return await invoke<TelegramLogEntry[]>('list_telegram_logs'); } catch { return []; }
}
// Telegram bots (multiple): each is long-polled and routed to an agent.
export type TelegramBot = { id: string; name: string; agentId: string; terminalId: string; allowedUsers: (string | number)[]; enabled: boolean; token: string; webhookRegistered: boolean; capture?: boolean; destinationId?: string; ownerChatId?: string };
export async function listTelegramBots(): Promise<TelegramBot[]> {
  try { return await invoke<TelegramBot[]>('list_telegram_bots'); } catch { return []; }
}
export async function saveTelegramBot(bot: TelegramBot): Promise<string> {
  return await invoke<string>('save_telegram_bot', { bot });
}
export async function deleteTelegramBot(id: string): Promise<void> {
  await invoke('delete_telegram_bot', { id });
}
// Sends a one-off reminder to the bot's bound chat (confirms the reminder path).
export async function sendTelegramTest(botId: string): Promise<void> {
  await invoke('telegram_send_test', { botId });
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
// The re-file pass: docs in the knowledge base that are really memories or todos.
export type RefileProposal = { docId: string; title: string; content: string; destination: 'memory' | 'items'; kind: string; dueAt: string | null };
export async function refileKnowledgePreview(): Promise<RefileProposal[]> {
  try { return await invoke<RefileProposal[]>('refile_knowledge_preview'); } catch { return []; }
}
export async function refileKnowledgeApply(proposals: RefileProposal[]): Promise<number> {
  return await invoke<number>('refile_knowledge_apply', { proposals });
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
// Scopes that actually hold memory (agent scopes included) — the graph reads this
// rather than the namespace registry, which only tracks projects.
export async function amListScopes(): Promise<unknown> {
  try { return await invoke('am_list_scopes'); } catch { return []; }
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

// The brain map (personal context engine). Graph reads nodes + edges; the
// observations tab lists inferred patterns with their evidence.
export async function brainGraph(): Promise<BrainGraph> {
  try { return await invoke<BrainGraph>('brain_graph'); } catch { return { nodes: [], edges: [] }; }
}
export async function brainNode(id: string): Promise<unknown> {
  try { return await invoke('brain_node', { id }); } catch { return null; }
}
export async function brainStats(): Promise<unknown> {
  try { return await invoke('brain_stats'); } catch { return null; }
}
export async function brainForgetNode(id: string): Promise<void> {
  await invoke('brain_forget_node', { id });
}
export async function brainListObservations(status?: string): Promise<BrainObservation[]> {
  try { return await invoke<BrainObservation[]>('brain_list_observations', { status: status ?? null }); } catch { return []; }
}
export async function brainAcceptObservation(id: string): Promise<void> {
  await invoke('brain_accept_observation', { id });
}
export async function brainDismissObservation(id: string): Promise<void> {
  await invoke('brain_dismiss_observation', { id });
}
export async function brainCorrectObservation(id: string, patch: { body?: string; confidence?: number }): Promise<void> {
  await invoke('brain_correct_observation', { id, patch });
}
export async function brainReindex(): Promise<number> {
  return await invoke<number>('brain_reindex');
}
export async function brainReflectNow(): Promise<number> {
  return await invoke<number>('brain_reflect_now');
}
// Hybrid retrieval — the same service chat, agents and workflows use. `rerank`
// asks the `reranker` model role to reorder the top hits (a no-op when unset).
export async function retrieveContext(query: string, scope?: string, modes?: string[], budget?: number, rerank?: boolean): Promise<{ items: unknown[]; text: string }> {
  try { return await invoke<{ items: unknown[]; text: string }>('retrieve_context', { query, scope: scope ?? null, modes: modes ?? null, budget: budget ?? null, rerank: rerank ?? null }); } catch { return { items: [], text: '' }; }
}
// The evaluation harness. `quality` scores a labelled fixture (precision/recall/MRR
// per case, plus duplicate + extraction reliability); `live` is a read-only
// measurement of the real store's scan size, vector-scan latency and context size.
export type BrainEval = {
  quality: {
    cases: number; precision: number; recall: number; mrr: number; worst_case: string | null;
    results: { case: string; modes: string[]; hits: number; precision: number; recall: number; mrr: number; micros: number; context_tokens_est: number }[];
    reliability: {
      duplicates: { ok: boolean; same_ref_nodes: number; items: number };
      extraction: { ok: boolean; recovered: number; samples: number; lost: number };
    };
  };
  live: {
    nodes: number; embeddings: number; fts: boolean; guidance: string;
    vector_scan: { dim: number; vectors: number; micros: number; per_vector_ns: number; top_similarity: number };
    context: { hits: number; chars: number; tokens_est: number; budget_tokens: number };
  };
};
export async function brainEval(): Promise<BrainEval | null> {
  try { return await invoke<BrainEval>('brain_eval'); } catch { return null; }
}
// The backend announces newly drafted observations so the mind doesn't have to be
// opened to learn there is something to review.
export type BrainNotice = { kind: string; count: number; title: string };
export function onBrainNotice(cb: (n: BrainNotice) => void): () => void {
  let unlisten: (() => void) | null = null;
  let disposed = false;
  listen<BrainNotice>('brain-notice', (e) => cb(e.payload))
    .then((un) => { if (disposed) un(); else unlisten = un; })
    .catch(() => {});
  return () => { disposed = true; unlisten?.(); };
}
// Model roles (classifier / embedding).
export async function listModelRoles(): Promise<ModelRoles> {
  try { return await invoke<ModelRoles>('list_model_roles'); } catch { return {}; }
}
export async function setModelRole(role: string, modelId: string): Promise<void> {
  await invoke('set_model_role', { role, modelId });
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

// Provider adapters: built-in (read-only) + custom (OpenAI-compatible).
export type ProviderDraft = ProviderRecord & { apiKey?: string; clearKey?: boolean };
export async function listProviders(): Promise<ProviderRecord[]> {
  try { return await invoke<ProviderRecord[]>('list_providers'); } catch { return []; }
}
export async function saveProvider(provider: ProviderDraft): Promise<void> {
  await invoke('save_provider', { provider });
}
export async function deleteProvider(id: string): Promise<void> {
  await invoke('delete_provider', { id });
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

// Runs one action of a tool through the same builder the runtime uses, so what the
// form shows is what an agent would get. `action` names a suite action; omit it for
// a single-endpoint tool.
export async function testTool(tool: Tool, action?: string, args?: Record<string, unknown>): Promise<string> {
  return await invoke<string>('test_tool', { tool, action: action ?? null, args: args ?? null });
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

// Import skills from a folder (or a single .md file) and export them back, as
// `SKILL.md` files in the Agent Skills format.
export type SkillImportReport = { imported: number; updated: number; skipped: number; messages: string[] };
export type SkillExportReport = { dir: string; count: number };

export async function importSkills(path: string): Promise<SkillImportReport> {
  return await invoke<SkillImportReport>('import_skills', { path });
}

// Drag-and-drop hands back file contents, not a path, so those go through a
// separate command.
export type SkillDraft = { name: string; content: string };

export async function importSkillFiles(files: SkillDraft[]): Promise<SkillImportReport> {
  return await invoke<SkillImportReport>('import_skill_files', { files });
}

export async function exportSkills(path: string): Promise<SkillExportReport> {
  return await invoke<SkillExportReport>('export_skills', { path });
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

// Destinations: named places (an MCP server + tool + arg template) an item can be
// pushed to — how several Notion accounts each receive captured ideas.
export type Destination = {
  id: string; name: string; serverId: string; tool: string; argTemplate: string; enabled: boolean;
  createdAt: string; updatedAt: string;
};
export const DEFAULT_ARG_TEMPLATE = '{\n  "title": "{{title}}",\n  "content": "{{body}}"\n}';
export async function listDestinations(): Promise<Destination[]> {
  try { return await invoke<Destination[]>('list_destinations'); } catch { return []; }
}
export async function saveDestination(destination: Destination): Promise<Destination> {
  return await invoke<Destination>('save_destination', { destination });
}
export async function deleteDestination(id: string): Promise<void> {
  await invoke('delete_destination', { id });
}
export async function destinationTools(serverId: string): Promise<string[]> {
  try { return await invoke<string[]>('destination_tools', { serverId }); } catch { return []; }
}
export async function testDestination(destination: Destination): Promise<string> {
  return await invoke<string>('test_destination', { destination });
}
export async function pushItem(id: string, destinationId: string): Promise<string> {
  return await invoke<string>('push_item', { id, destinationId });
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

export async function updateTaskResult(id: string, result: string): Promise<void> {
  await invoke('update_task_result', { id, result });
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

// App settings: background/tray mode + launch at login.
export type CliShortcut = { name: string; command: string; cwd: string };
export type AppSettings = {
  runInBackground: boolean; launchAtLogin: boolean; autostartEnabled: boolean;
  defaultShell: string; cliShortcuts: CliShortcut[]; autonomy: boolean;
  desktopNotifications: boolean; voiceHotkey: string; voiceTtsMode: string;
};
export async function getAppSettings(): Promise<AppSettings> {
  try { return await invoke<AppSettings>('get_app_settings'); } catch { return { runInBackground: true, launchAtLogin: false, autostartEnabled: false, defaultShell: '', cliShortcuts: [], autonomy: true, desktopNotifications: true, voiceHotkey: 'CommandOrControl+Shift+Space', voiceTtsMode: 'auto' }; }
}
export async function setRunInBackground(enabled: boolean): Promise<void> {
  await invoke('set_run_in_background', { enabled });
}
export async function setLaunchAtLogin(enabled: boolean): Promise<void> {
  await invoke('set_launch_at_login', { enabled });
}
export async function setAutonomy(enabled: boolean): Promise<void> {
  await invoke('set_autonomy', { enabled });
}
export async function setDefaultShell(shell: string): Promise<void> {
  await invoke('set_default_shell', { shell });
}
export async function setCliShortcuts(shortcuts: CliShortcut[]): Promise<void> {
  await invoke('set_cli_shortcuts', { shortcuts });
}
export async function setDesktopNotifications(enabled: boolean): Promise<void> {
  await invoke('set_desktop_notifications', { enabled });
}
// 'auto' | 'system' | 'provider'
export async function setVoiceTtsMode(mode: string): Promise<void> {
  await invoke('set_voice_tts_mode', { mode });
}
export async function setVoiceHotkey(accelerator: string): Promise<void> {
  await invoke('set_voice_hotkey', { accelerator });
}

// Live voice mode: transcribe one captured utterance, and synthesise a spoken
// reply (base64 mp3, or null when no TTS model is configured).
export async function voiceTranscribe(name: string, mime: string, data: string): Promise<string> {
  return await invoke<string>('voice_transcribe', { name, mime, data });
}
export async function voiceSpeak(text: string): Promise<string | null> {
  return await invoke<string | null>('voice_speak', { text });
}

// The global hotkey asks the frontend to toggle live voice mode.
export function onVoiceToggle(cb: () => void): () => void {
  let unlisten: (() => void) | null = null;
  let disposed = false;
  listen<unknown>('voice-toggle', () => cb())
    .then((un) => { if (disposed) un(); else unlisten = un; })
    .catch(() => {});
  return () => { disposed = true; unlisten?.(); };
}

// Capture / Lumin: notes, todos, reminders, goals, links and habits.
export type Item = {
  id: string; kind: string; title: string; body: string; url: string; tags: string[];
  collection: string; status: string; due_at: string | null; cadence: string; streak: number;
  last_done_at: string | null; project_id: string; fired_at: string | null;
  source: string; owner: string; created_at: string; updated_at: string;
};
export const ITEM_KINDS = ['note', 'todo', 'reminder', 'goal', 'resource', 'habit'] as const;
// `owner` filters by who filed the item: undefined = any, '' = you, else an agent id.
export async function itemList(kind?: string, owner?: string): Promise<Item[]> {
  return await invoke<Item[]>('item_list', { kind: kind ?? null, owner: owner ?? null });
}
export async function itemSave(item: Item): Promise<Item> {
  return await invoke<Item>('item_save', { item });
}
export async function itemDelete(id: string): Promise<void> {
  await invoke('item_delete', { id });
}
export async function itemToggle(id: string): Promise<string> {
  return await invoke<string>('item_toggle', { id });
}
export async function itemDue(upcoming = false): Promise<Item[]> {
  return await invoke<Item[]>('item_due', { upcoming });
}
export async function reminderSnooze(id: string, minutes = 10): Promise<void> {
  await invoke('reminder_snooze', { id, minutes });
}

// Learning layer: insights the assistant proposes, which the user accepts/dismisses.
export type Insight = { id: string; kind: string; title: string; body: string; confidence: number; status: string; created_at: string; updated_at: string };
export async function insightList(status?: string): Promise<Insight[]> {
  try { return await invoke<Insight[]>('insight_list', { status: status ?? null }); } catch { return []; }
}
export async function insightAccept(id: string): Promise<void> { await invoke('insight_accept', { id }); }
export async function insightDismiss(id: string): Promise<void> { await invoke('insight_dismiss', { id }); }
export async function reflectNow(): Promise<number> { return await invoke<number>('reflect_now'); }

// The notification queue: reminders, insights and finished work, persisted so the
// inbox survives a reload (and the window sitting on the tray).
export type Notification = {
  id: number; kind: string; title: string; body: string;
  refKind: string; refId: string; createdAt: string; readAt: string | null; dismissedAt: string | null;
};
export async function listNotifications(unreadOnly = false): Promise<Notification[]> {
  try { return await invoke<Notification[]>('list_notifications', { unreadOnly }); } catch { return []; }
}
export async function notificationUnreadCount(): Promise<number> {
  try { return await invoke<number>('notification_unread_count'); } catch { return 0; }
}
export async function markNotificationRead(id: number): Promise<void> { await invoke('mark_notification_read', { id }); }
export async function dismissNotification(id: number): Promise<void> { await invoke('dismiss_notification', { id }); }
export async function markAllNotificationsRead(): Promise<void> { await invoke('mark_all_notifications_read'); }
export async function clearNotifications(): Promise<void> { await invoke('clear_notifications'); }

// Fires when the backend queues a new notification (reminder, insight, finished work).
export function onNotification(cb: (n: { id: number; kind: string; title: string; body: string; refKind: string; refId: string }) => void): () => void {
  let unlisten: (() => void) | null = null;
  let disposed = false;
  listen<{ id: number; kind: string; title: string; body: string; refKind: string; refId: string }>('lumin-notification', (e) => cb(e.payload))
    .then((un) => { if (disposed) un(); else unlisten = un; })
    .catch(() => {});
  return () => { disposed = true; unlisten?.(); };
}

// Fires when the backend finishes a background task. The chat that asked for it
// uses this to run the agent's follow-up summary. `assignedAgent` is the agent the
// work actually went to, which is how a delegated agent's window gets opened.
export type TaskFinished = { taskId: string; chatAgentId: string; assignedAgent?: string; sessionId: string; status: string; result: string };
export function onTaskFinished(cb: (t: TaskFinished) => void): () => void {
  let unlisten: (() => void) | null = null;
  let disposed = false;
  listen<TaskFinished>('lumin-task-finished', (e) => cb(e.payload))
    .then((un) => { if (disposed) un(); else unlisten = un; })
    .catch(() => {});
  return () => { disposed = true; unlisten?.(); };
}

// Fires when the backend starts a background task, so the activity panel can show
// it while the chat carries on.
export type TaskStarted = { taskId: string; chatAgentId: string; assignedAgent?: string; sessionId: string; kind: string; tool: string; label: string };
export function onTaskStarted(cb: (t: TaskStarted) => void): () => void {
  let unlisten: (() => void) | null = null;
  let disposed = false;
  listen<TaskStarted>('lumin-task-started', (e) => cb(e.payload))
    .then((un) => { if (disposed) un(); else unlisten = un; })
    .catch(() => {});
  return () => { disposed = true; unlisten?.(); };
}

// Fires when an agent is created or deleted outside the UI — the Manager does both
// from a chat turn. The roster only loads once at startup, so without this a new
// agent would not appear anywhere until the app was reloaded.
export type AgentsChanged = { id: string; deleted?: boolean };
export function onAgentsChanged(cb: (a: AgentsChanged) => void): () => void {
  let unlisten: (() => void) | null = null;
  let disposed = false;
  listen<AgentsChanged>('lumin-agents-changed', (e) => cb(e.payload))
    .then((un) => { if (disposed) un(); else unlisten = un; })
    .catch(() => {});
  return () => { disposed = true; unlisten?.(); };
}

// Identity: who the assistant is and who it's talking to.
export type Identity = { userName: string; botName: string; persona: string; tone: string; timezone: string; verbosity: number };
export async function getIdentity(): Promise<Identity> {
  try { return await invoke<Identity>('get_identity'); } catch { return { userName: '', botName: '', persona: '', tone: '', timezone: '', verbosity: 0 }; }
}
export async function setIdentity(identity: Identity): Promise<void> {
  await invoke('set_identity', { identity });
}

// Embedded mini browser: a real webview the React panel paints over its viewport.
// These drive it; `onBrowserEvent` reports its state so the drawer can slide open
// whenever an agent uses browser control.
export type BrowserState = { visible: boolean; url: string; title: string; loading: boolean };
export async function browserOpen(url?: string): Promise<void> {
  try { await invoke('browser_open', { url: url ?? null }); } catch { /* browser preview: no Tauri backend */ }
}
export async function browserNavigate(url: string): Promise<void> {
  await invoke('browser_navigate', { url });
}
export async function browserBack(): Promise<void> { await invoke('browser_back'); }
export async function browserForward(): Promise<void> { await invoke('browser_forward'); }
export async function browserReload(): Promise<void> { await invoke('browser_reload'); }
export async function browserClose(): Promise<void> { try { await invoke('browser_close'); } catch { /* no backend */ } }
export async function browserSetBounds(x: number, y: number, width: number, height: number): Promise<void> {
  try { await invoke('browser_set_bounds', { x, y, width, height }); } catch { /* no backend */ }
}
// Keeps pages in the in-app browser in step with the app theme: this drives the
// window colour scheme, which the webview maps to prefers-color-scheme.
export async function browserSetTheme(theme: 'light' | 'dark'): Promise<void> {
  try { await invoke('browser_set_theme', { theme }); } catch { /* no backend */ }
}
export async function browserState(): Promise<{ open: boolean; visible: boolean; url: string }> {
  try { return await invoke('browser_state'); } catch { return { open: false, visible: false, url: '' }; }
}
export function onBrowserEvent(cb: (s: BrowserState) => void): () => void {
  let unlisten: (() => void) | null = null;
  let disposed = false;
  listen<BrowserState>('lup-browser', (e) => cb(e.payload))
    .then((un) => { if (disposed) un(); else unlisten = un; })
    .catch(() => {});
  return () => { disposed = true; unlisten?.(); };
}

// Resource / connection / adapter layer. An adapter is a descriptor (built-in or
// seeded from code), a connection is a configured instance with credentials, and
// a resource is a concrete external object a connection exposes.
export type Adapter = {
  id: string; name: string; category: string; provider: string; auth: string;
  baseUrl: string; caps: Record<string, boolean>;
  fields: { key: string; label: string; secret: boolean }[];
  actions: { name: string; description: string }[];
  builtin: boolean;
};
export type Connection = {
  id: string; adapterId: string; name: string; category: string;
  enabled: boolean; connected: boolean; builtin: boolean;
};
export type Resource = {
  id: string; connectionId: string; kind: string; name: string; externalId: string;
  url: string; meta: Record<string, unknown>; createdAt: string; updatedAt: string;
};
export async function listAdapters(): Promise<Adapter[]> {
  try { return await invoke<Adapter[]>('list_adapters'); } catch { return []; }
}
export async function listConnections(): Promise<Connection[]> {
  try { return await invoke<Connection[]>('list_connections'); } catch { return []; }
}
export async function listResources(): Promise<Resource[]> {
  try { return await invoke<Resource[]>('list_resources'); } catch { return []; }
}
export async function saveResource(resource: Resource): Promise<Resource> {
  return await invoke<Resource>('save_resource', { resource });
}
export async function deleteResource(id: string): Promise<void> {
  await invoke('delete_resource', { id });
}
