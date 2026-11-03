import type { ModelType } from './types';

// The kind of work a model does. This is separate from the provider (who serves
// it): one provider can host a chat model, an image model and a transcriber, and
// the picker groups by this so the right kind is obvious at a glance.
//
// Order is deliberate — chat-capable kinds first, then generation, then speech —
// because that is the order the composer lists groups in.
export const MODEL_TYPES: { value: ModelType; label: string; hint: string }[] = [
  { value: 'llm', label: 'llm', hint: 'text chat and reasoning' },
  { value: 'vision', label: 'vision', hint: 'understands images' },
  { value: 'image', label: 'image', hint: 'generates images' },
  { value: 'video', label: 'video', hint: 'generates or understands video' },
  { value: 'media', label: 'media', hint: 'handles media files' },
  { value: 'multimedia', label: 'multimedia', hint: 'text, image and audio in one model' },
  { value: 'stt', label: 'stt', hint: 'speech to text' },
  { value: 'tts', label: 'tts', hint: 'text to speech' },
];

export const MODEL_TYPE_LABEL: Record<ModelType, string> = Object.fromEntries(
  MODEL_TYPES.map((t) => [t.value, t.label]),
) as Record<ModelType, string>;

// Kinds a chat turn can actually run on. The composer still shows every enabled
// model so a non-chat model can be switched to (and pinned to roles/nodes), but
// this marks which ones produce a text reply today.
export const CHAT_MODEL_TYPES: ModelType[] = ['llm', 'vision', 'media', 'multimedia'];

export const isChatModel = (kind: ModelType | undefined): boolean => CHAT_MODEL_TYPES.includes(kind ?? 'llm');
