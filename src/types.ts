// Provider ids are open-ended: the built-ins plus any adapter the user adds in
// Settings → Providers.
export type Provider = string;
// What a provider can do. `chat` is always true; the rest gate the model roles.
export type ProviderCaps = { chat: boolean; vision: boolean; embeddings: boolean; transcription: boolean; speech: boolean };
// A provider adapter. Built-ins are read-only; custom ones are editable.
export type ProviderRecord = {
  id: string; label: string; baseUrl: string; auth: string; authHeader: string;
  extraHeaders: Record<string, string>; requiresKey: boolean; reasoning: string;
  providerRouting: boolean; caps: ProviderCaps; hasKey: boolean; builtin: boolean;
};
// Per-model reasoning control. 'auto' sends nothing at all to the provider, which
// is the safe default: models without reasoning support never see the parameter.
export type Reasoning = 'auto' | 'off' | 'low' | 'medium' | 'high';
export type Agent = { id: string; name: string; objective: string; model: string; reasoning: Reasoning; toolIds: string[]; integrations: string[]; skillIds: string[]; memory: boolean; permissions: string[]; homePath: string; color: string; x: number; y: number; isManager?: boolean; description?: string; persona?: string; pinned?: boolean; avatar?: string };
export type RunEvent = { time: string; type: 'thought' | 'tool' | 'result'; title: string; detail?: string };
export type Run = { id: string; agentId: string; startedAt: string; endedAt?: string; status: 'running' | 'completed' | 'failed' | 'cancelled'; model: string; input: string; events: RunEvent[]; output?: string; promptTokens?: number; completionTokens?: number };
export type Edge = { id: string; from: string; to: string };
// What a model is for. Separate from `Provider` (who serves it): one provider can
// host a chat model, an image model and a transcriber. `llm` is the default so
// existing rows keep working unchanged.
export type ModelType = 'llm' | 'vision' | 'image' | 'video' | 'media' | 'multimedia' | 'stt' | 'tts';
export type ModelConfig = { id: string; provider: Provider; label: string; model: string; kind?: ModelType; voice?: string; host?: string; apiKey?: string; hasKey?: boolean; clearKey?: boolean; enabled: boolean; reasoning: Reasoning; vision?: boolean; transcribes?: boolean };
export type IntegrationAction = { name: string; description: string };
export type Integration = { id: string; name: string; provider: string; enabled: boolean; connected: boolean; config: Record<string, unknown>; actions: IntegrationAction[] };
export type ToolParam = { name: string; type: string; description: string; required: boolean };
export type ToolConfig = {
  [key: string]: string | number | boolean | undefined | ToolParam[] | { name: string; value: string }[];
  method?: string;
  url?: string;
  body?: string;
  name?: string;
  serverName?: string;
  params?: ToolParam[];
  headers?: { name: string; value: string }[];
};
export type Tool = { id: string; name: string; kind: string; integrationId: string; description: string; enabled: boolean; config: ToolConfig };
export type Skill = { id: string; name: string; description: string; content: string; docs: string; scriptIds: string[] };
// A reusable runnable app: a shell command or an internal model prompt.
export type Script = { id: string; name: string; description: string; kind: 'command' | 'prompt'; command: string; cwd: string; prompt: string; model: string; updatedAt: string };
// A project groups conversations and scopes their memory.
export type Project = { id: string; name: string; description: string; createdAt: string; updatedAt: string };

export type WorkflowNodeType = 'agent' | 'subagent' | 'loop' | 'checker' | 'integration' | 'gate' | 'trigger' | 'llm' | 'script' | 'retrieve_context';
export type WorkflowNode = {
  id: string; type: WorkflowNodeType; agentId?: string; toolId?: string; label: string; x: number; y: number;
  config?: Record<string, unknown>;
};
// A wire leaves a named output port and lands on a named input port, so branch
// outcomes (pass/fail, true/false, success/error) survive a save round-trip.
export type WorkflowEdge = { id: string; from: string; to: string; fromPort?: string; toPort?: string };
export type Workflow = { id: string; name: string; nodes: WorkflowNode[]; edges: WorkflowEdge[]; updatedAt: string };
export type WorkflowRunStep = { nodeId: string; nodeLabel: string; output: string; promptTokens: number; completionTokens: number };
export type WorkflowRunResult = { steps: WorkflowRunStep[]; finalOutput: string; totalPromptTokens: number; totalCompletionTokens: number };
export type Task = { id: string; requester: string; assignedAgent: string; status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'; input: string; context: string; result?: string | null; createdAt: string; completedAt?: string | null };

// 'builder' is the workflow canvas, kept distinct from the 'workflows' list so
// opening a workflow is a real navigation step the chrome can go back from.
export type View = 'home' | 'graph' | 'agents' | 'workflows' | 'builder' | 'manager' | 'tasks' | 'runs' | 'telegram' | 'terminal' | 'items' | 'agent' | 'workshop' | 'settings' | 'memory' | 'knowledge';

// What the topbar "+" creates. Left-click runs the default; right-click offers
// the whole list and lets you pick which one is the default.
export type CreateKind = 'agent' | 'workflow' | 'skill' | 'script' | 'project' | 'tool' | 'model';

export type DrawerForm =
  | { kind: 'tool'; editing: Tool; isNew: boolean }
  | { kind: 'model'; editing: ModelConfig; isNew: boolean }
  | { kind: 'skill'; editing: Skill; isNew: boolean }
  | { kind: 'script'; editing: Script; isNew: boolean }
  | { kind: 'project'; editing: Project; isNew: boolean }
  | null;

export type ChatMessage = { role: 'user' | 'assistant' | 'tool' | 'thought'; content: string; detail?: string; time: string };

// A file handed to a chat turn. `data` is base64 without the `data:` prefix, the
// same shape the vision tools produce, so the backend can put it straight into a
// provider request.
export type Attachment = { name: string; mime: string; data: string };

// The brain map: the user's life as connected nodes + typed edges. `provenance`
// is 'stated' (the user said it) or 'inferred' (the system guessed, with a
// confidence) — never blurred together.
export type BrainNode = {
  id: string; kind: string; scope: string; label: string;
  refKind?: string; refId?: string; provenance: string; status: string;
  salience?: number;
  // Per-dimension attention signals (allocation, dormancy, momentum, drift…).
  attention?: Record<string, number>;
};
export type BrainEdge = { id: string; from: string; to: string; relation: string; provenance: string; confidence: number };
export type BrainGraph = { nodes: BrainNode[]; edges: BrainEdge[] };
// An inferred pattern with evidence — a proposal the user can accept, dismiss or correct.
export type BrainObservation = { id: string; scope: string; dimension: string; kind: string; title: string; body: string; confidence: number; evidence: unknown; status: string; firstSeenAt: number; lastSeenAt: number };
// A model role (classifier / embedding): the pinned model and the one actually resolved.
export type ModelRoleInfo = { modelId: string; resolved: string | null };
export type ModelRoles = Record<string, ModelRoleInfo>;

// One real step in a chat turn, fed live from the backend so the pending bubble
// reports what the agent is doing rather than a fixed "thinking" label.
// `streaming` marks text still arriving token by token (the model's reasoning),
// so later chunks grow that step instead of adding a line per token.
export type ChatStep = { kind: 'think' | 'tool' | 'write' | 'wait'; text: string; time: string; streaming?: boolean };

export type ExecutionResult = { output: string; events: RunEvent[]; runId?: string; promptTokens?: number; completionTokens?: number };

export const emptyTool = (): Tool => ({ id: '', name: '', kind: 'api', integrationId: 'http', description: '', enabled: true, config: { method: 'GET', url: '', headers: [], body: '', params: [] } });
export const emptyModel = (): ModelConfig => ({ id: '', provider: 'groq', label: '', model: '', kind: 'llm', host: '', apiKey: '', enabled: true, reasoning: 'auto' });
export const emptySkill = (): Skill => ({ id: '', name: '', description: '', content: '', docs: '', scriptIds: [] });
export const emptyScript = (): Script => ({ id: '', name: '', description: '', kind: 'command', command: '', cwd: '', prompt: '', model: '', updatedAt: '' });
export const emptyProject = (): Project => ({ id: '', name: '', description: '', createdAt: '', updatedAt: '' });
