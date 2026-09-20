import { Dropdown } from './Dropdown';
import { useModelsStore } from '../../hooks/useModels';
import type { Reasoning } from '../../types';

// Quick access to the current model's reasoning level, sitting right beside the
// composer so it doesn't require a trip through Settings → Models → Edit.
//
// The setting is stored per model, not per agent, so changing it here changes it
// for every agent that uses this model.
const OPTIONS: { value: Reasoning; label: string }[] = [
  { value: 'auto', label: 'Reasoning: auto' },
  { value: 'off', label: 'Reasoning: off' },
  { value: 'low', label: 'Reasoning: low' },
  { value: 'medium', label: 'Reasoning: medium' },
  { value: 'high', label: 'Reasoning: high' },
];

export function ReasoningPicker({ modelId }: { modelId: string }) {
  const model = useModelsStore((s) => s.models.find((m) => m.id === modelId));
  // A model the store doesn't know about (deleted, or a seed that was never
  // loaded) simply has no picker rather than a broken control.
  if (!model) return null;

  return (
    <div className="w-[152px] shrink-0">
      <Dropdown
        value={model.reasoning}
        options={OPTIONS}
        onChange={(v) => void useModelsStore.getState().saveModel({ ...model, reasoning: v as Reasoning })}
      />
    </div>
  );
}
