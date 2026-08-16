import type { ModelConfig, Tool } from './types';
export const models: ModelConfig[] = [{ id:'ollama:qwen3:8b', provider:'ollama', label:'Qwen3 8B', model:'qwen3:8b', host:'http://localhost:11434', enabled:true }, { id:'openrouter:anthropic/claude-3.5-haiku', provider:'openrouter', label:'Claude 3.5 Haiku', model:'anthropic/claude-3.5-haiku', enabled:false }];
export const initialTools: Tool[] = [
  { id:'http-get', name:'HTTP GET', kind:'http_get', integrationId:'http', description:'Make a permission-scoped GET request and return the response body.', enabled:true, config:{} },
  { id:'read-file', name:'Read File', kind:'read_file', integrationId:'builtin', description:'Read a file from the agent\'s isolated files directory.', enabled:true, config:{} },
  { id:'write-file', name:'Write File', kind:'write_file', integrationId:'builtin', description:'Write text content to a file in the agent\'s isolated files directory.', enabled:true, config:{} },
];
