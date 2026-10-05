import type { Reasoning } from './types';

// The reasoning dial's stops, shared by the chat composer and the model form so
// the control reads identically everywhere.
export const REASONING_STOPS: { value: Reasoning; label: string }[] = [
  { value: 'auto', label: 'auto — leave it to the model' },
  { value: 'off', label: 'off — answer directly' },
  { value: 'low', label: 'low effort' },
  { value: 'medium', label: 'medium effort' },
  { value: 'high', label: 'high effort' },
];
