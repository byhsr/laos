export type Provider = 'ollama' | 'openrouter' | 'groq';
export type Agent = { id: string; name: string; objective: string; model: string; toolIds: string[]; integrations: string[]; memory: boolean; permissions: string[]; homePath: string; color: string; x: number; y: number; isManager?: boolean; description?: string; persona?: string };
export type RunEvent = { time: string; type: 'thought' | 'tool' | 'result'; title: string; detail?: string };
export type Run = { id: string; agentId: string; startedAt: string; endedAt?: string; status: 'running' | 'completed' | 'failed' | 'cancelled'; model: string; input: string; events: RunEvent[]; output?: string; promptTokens?: number; completionTokens?: number };
export type Edge = { id: string; from: string; to: string };
export type ModelConfig = { id: string; provider: Provider; label: string; model: string; host?: string; apiKey?: string; enabled: boolean };
export type IntegrationAction = { name: string; description: string };
export type Integration = { id: string; name: string; provider: string; enabled: boolean; connected: boolean; config: Record<string, unknown>; actions: IntegrationAction[] };
export type ToolParam = { name: string; type: string; description: string; required: boolean };
export type ToolConfig = {
  [key: string]: string | number | boolean | undefined | ToolParam[] | { name: string; value: string }[];
  baseUrl?: string;
  apiKey?: string;
  method?: string;
  url?: string;
  body?: string;
  name?: string;
  params?: ToolParam[];
  headers?: { name: string; value: string }[];
};
export type Tool = { id: string; name: string; kind: string; integrationId: string; description: string; enabled: boolean; config: ToolConfig };

export type WorkflowNodeType = 'agent' | 'subagent' | 'loop' | 'checker' | 'integration' | 'gate' | 'trigger';
export type WorkflowNode = {
  id: string; type: WorkflowNodeType; agentId?: string; toolId?: string; label: string; x: number; y: number;
  config?: Record<string, unknown>;
};
export type WorkflowEdge = { id: string; from: string; to: string };
export type Workflow = { id: string; name: string; nodes: WorkflowNode[]; edges: WorkflowEdge[]; updatedAt: string };
export type WorkflowRunStep = { nodeId: string; nodeLabel: string; output: string; promptTokens: number; completionTokens: number };
export type WorkflowRunResult = { steps: WorkflowRunStep[]; finalOutput: string; totalPromptTokens: number; totalCompletionTokens: number };
export type Task = { id: string; requester: string; assignedAgent: string; status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'; input: string; context: string; result?: string | null; createdAt: string; completedAt?: string | null };

export type View = 'home' | 'agents' | 'workflows' | 'manager' | 'tasks' | 'runs' | 'agent' | 'integrations' | 'tools' | 'models' | 'knowledge' | 'settings';

export type DrawerForm =
  | { kind: 'tool'; editing: Tool; isNew: boolean }
  | { kind: 'model'; editing: ModelConfig; isNew: boolean }
  | null;

export type ChatMessage = { role: 'user' | 'assistant' | 'tool' | 'thought'; content: string; detail?: string; time: string };

export type ExecutionResult = { output: string; events: RunEvent[]; runId?: string; promptTokens?: number; completionTokens?: number };

export const emptyTool = (): Tool => ({ id: '', name: '', kind: 'api', integrationId: 'http', description: '', enabled: true, config: { method: 'GET', url: '', headers: [], body: '', params: [] } });
export const emptyModel = (): ModelConfig => ({ id: '', provider: 'groq', label: '', model: '', host: '', apiKey: '', enabled: true });
