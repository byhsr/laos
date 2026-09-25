import { Bot, GitBranch, Repeat, ShieldCheck, Webhook } from 'lucide-react';
import type { WorkflowNodeType } from '../../types';

// The builder's vocabulary. Each type is legible from its glyph alone, so the
// palette renders icons and the description lives in a tooltip — colour stays
// reserved for port tones.
export const NODE_TYPES: { type: WorkflowNodeType; label: string; icon: React.ReactNode; desc: string }[] = [
  { type: 'trigger', label: 'trigger', icon: <Webhook size={13} />, desc: 'Workflow entry point' },
  { type: 'agent', label: 'agent', icon: <Bot size={13} />, desc: 'Run an agent' },
  { type: 'subagent', label: 'subagent', icon: <GitBranch size={13} />, desc: 'Delegate to a sub-agent' },
  { type: 'loop', label: 'loop', icon: <Repeat size={13} />, desc: 'Repeat until done' },
  { type: 'checker', label: 'checker', icon: <ShieldCheck size={13} />, desc: 'Validate output' },
  { type: 'integration', label: 'integration', icon: <Webhook size={13} />, desc: 'Call a tool/integration' },
  { type: 'gate', label: 'gate', icon: <GitBranch size={13} />, desc: 'Conditional branch' },
];

export const TYPE_META: Record<WorkflowNodeType, { label: string; icon: React.ReactNode; desc: string }> =
  Object.fromEntries(NODE_TYPES.map((n) => [n.type, { label: n.label, icon: n.icon, desc: n.desc }])) as
  Record<WorkflowNodeType, { label: string; icon: React.ReactNode; desc: string }>;
