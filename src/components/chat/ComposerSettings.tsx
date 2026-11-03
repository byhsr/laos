import { useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, Plus } from 'lucide-react';
import { OPTION_ROW, useAnchoredPosition, useDismiss } from '../ui/popover';
import { tabCls } from '../ui/tabs';
import { Slider } from '../ui/Slider';
import { ModelLogo } from '../ui/ModelLogo';
import { useModelsStore } from '../../hooks/useModels';
import { MODEL_TYPES } from '../../modelTypes';
import { REASONING_STOPS } from '../../reasoning';
import type { Reasoning } from '../../types';

// Everything you can change about the next turn, behind one "+" next to send.
//
// Two tabs, each owning its surface: `models` is a scrolling list grouped by
// model kind, `reasoning` is the dial for the agent whose composer this is.
function Row({ label, active, onClick, icon }: { label: string; active: boolean; onClick: () => void; icon: React.ReactNode }) {
  return (
    <button type="button" role="menuitem" className={`${OPTION_ROW} ${active ? 'text-foreground' : ''}`} onClick={onClick}>
      {icon}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {active && <Check size={12} className="shrink-0 text-foreground" />}
    </button>
  );
}

export function ComposerSettings({ modelId, reasoning, onModelChange, onReasoningChange }: {
  modelId: string;
  reasoning: Reasoning;
  onModelChange: (modelId: string) => void;
  onReasoningChange: (reasoning: Reasoning) => void;
}) {
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<'models' | 'reasoning'>('models');
  const anchor = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const pos = useAnchoredPosition(open, anchor, { minWidth: 208 });
  useDismiss(open, () => setOpen(false), anchor, panel);

  const models = useModelsStore((s) => s.models);
  const enabled = models.filter((m) => m.enabled);
  // Group by kind so switching between an llm and, say, a vision or image model
  // is a deliberate, labelled move rather than a long undifferentiated list.
  const groups = MODEL_TYPES
    .map((t) => ({ ...t, models: enabled.filter((m) => (m.kind ?? 'llm') === t.value) }))
    .filter((g) => g.models.length > 0);

  return (
    <>
      <button
        ref={anchor}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label="chat settings"
        className={`focus-ring grid h-[30px] w-[30px] shrink-0 cursor-pointer place-items-center rounded-lg border transition-colors ${
          open ? 'border-border bg-background text-foreground' : 'border-transparent text-muted hover:bg-background hover:text-foreground'
        }`}
        onClick={() => setOpen((o) => !o)}
      >
        <Plus size={14} />
      </button>

      {open && pos && createPortal(
        <div
          ref={panel}
          role="menu"
          className="popover-shell flex max-h-[320px] min-w-[208px] flex-col overflow-hidden"
          style={{ top: pos.top, bottom: pos.bottom, left: pos.left }}
        >
          <div className="flex shrink-0 items-center gap-1 border-b border-border p-1.5">
            <button type="button" className={tabCls(tab === 'models')} onClick={() => setTab('models')}>models</button>
            <button type="button" className={tabCls(tab === 'reasoning')} onClick={() => setTab('reasoning')}>reasoning</button>
          </div>

          <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto p-1.5">
            {tab === 'models' ? (
              groups.length === 0
                ? <p className="m-0 px-2.5 py-1.5 font-mono text-[10px] text-muted">no models enabled</p>
                : groups.map((g) => (
                  <div key={g.value} className="pt-1 first:pt-0">
                    <p className="m-0 px-2.5 py-1 font-mono text-[10px] lowercase text-muted/70">{g.label}</p>
                    {g.models.map((m) => (
                      <Row
                        key={m.id}
                        label={m.label || m.model}
                        active={m.id === modelId}
                        icon={<ModelLogo model={m} size={13} />}
                        onClick={() => { setOpen(false); onModelChange(m.id); }}
                      />
                    ))}
                  </div>
                ))
            ) : (
              <Slider
                value={reasoning}
                stops={REASONING_STOPS}
                ariaLabel="reasoning"
                onChange={(v) => onReasoningChange(v as Reasoning)}
              />
            )}
          </div>
        </div>,
        document.body,
      )}
    </>
  );
}
