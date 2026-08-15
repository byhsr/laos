export type Provider = 'ollama' | 'openrouter';
export type Agent = { id: string; name: string; objective: string; model: string; toolIds: string[]; integrations: string[]; memory: boolean; permissions: string[]; homePath: string; color: string; x: number; y: number };
export type RunEvent = { time: string; type: 'thought' | 'tool' | 'result'; title: string; detail?: string };
export type Run = { id: string; agentId: string; startedAt: string; endedAt?: string; status: 'running' | 'completed' | 'failed' | 'cancelled'; model: string; input: string; events: RunEvent[]; output?: string; promptTokens?: number; completionTokens?: number };
export type Edge = { id: string; from: string; to: string };
export type ModelConfig = { id: string; provider: Provider; label: string; model: string; host?: string; apiKey?: string; enabled: boolean };
export type Integration = { id: string; name: string; kind: string; enabled: boolean; configured: boolean; description: string };
export type ToolParam = { name: string; type: string; description: string; required: boolean };
export type Tool = { id: string; name: string; kind: string; integrationId: string; description: string; enabled: boolean; config: Record<string, string> & { params?: ToolParam[]; headers?: { name: string; value: string }[] } };

export type View = 'home' | 'agents' | 'canvas' | 'runs' | 'agent' | 'integrations' | 'tools' | 'models' | 'settings';

export type DrawerForm =
  | { kind: 'tool'; editing: Tool; isNew: boolean }
  | { kind: 'model'; editing: ModelConfig; isNew: boolean }
  | null;

export type ChatMessage = { role: 'user' | 'assistant' | 'tool' | 'thought'; content: string; detail?: string; time: string };

export type ExecutionResult = { output: string; events: RunEvent[]; runId?: string; promptTokens?: number; completionTokens?: number };

export const emptyTool = (): Tool => ({ id: '', name: '', kind: 'api', integrationId: 'http', description: '', enabled: true, config: { method: 'GET', url: '', headers: [], body: '', params: [] } });
export const emptyModel = (): ModelConfig => ({ id: '', provider: 'openrouter', label: '', model: '', host: '', apiKey: '', enabled: true });
