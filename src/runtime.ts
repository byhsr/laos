import { Channel, invoke } from '@tauri-apps/api/core';
import type { Agent, Integration, ModelConfig, RunEvent, Task, Tool, Workflow, WorkflowRunResult } from './types';

// Streams a chat completion, calling onDelta with each token chunk.
export async function streamChat(agent: Agent, input: string, isManager: boolean, onDelta: (d: string) => void): Promise<void> {
  const channel = new Channel<string>();
  channel.onmessage = (d) => onDelta(d);
  await invoke('stream_chat', { agent, input, isManager, onEvent: channel });
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
