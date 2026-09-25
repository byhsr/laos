import { Fragment, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, ChevronDown } from 'lucide-react';
import { OPTION_ROW, useAnchoredPosition, useDismiss } from './popover';
import type { SelectOption } from './Select';

// Multi-select sibling of Select: identical trigger shell and identical popover
// panel, with checkboxes in the menu and a summary in the trigger. Picking an
// option keeps the menu open — it is a set, not a choice.
export function MultiDropdown({ values, options, onChange, placeholder = 'select…' }: {
  values: string[];
  options: SelectOption[];
  onChange: (v: string[]) => void;
  placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const pos = useAnchoredPosition(open, anchor, { matchWidth: true, minWidth: 220 });
  useDismiss(open, () => setOpen(false), anchor, panel);

  const selected = options.filter((o) => values.includes(o.value));
  const toggle = (v: string) => onChange(values.includes(v) ? values.filter((x) => x !== v) : [...values, v]);

  return (
    <div className="relative" ref={anchor}>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="focus-ring flex h-[30px] w-full cursor-pointer items-center justify-between gap-2 rounded-lg border border-border bg-surface px-2.5 text-left font-mono text-[11px] text-foreground transition-colors hover:bg-background"
      >
        <span className={`min-w-0 flex-1 truncate ${selected.length === 0 ? 'text-muted' : ''}`}>
          {selected.length === 0 ? placeholder : selected.map((s) => s.label).join(', ')}
        </span>
        <ChevronDown size={13} className={`shrink-0 text-muted transition-transform duration-150 ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && pos && createPortal(
        <div
          ref={panel}
          role="listbox"
          aria-multiselectable
          className="popover-shell max-h-[300px] overflow-x-hidden overflow-y-auto p-1.5"
          style={{ top: pos.top, bottom: pos.bottom, left: pos.left, width: pos.width }}
        >
          {options.length === 0 && (
            <p className="m-0 px-2.5 py-1.5 font-mono text-xs lowercase text-muted">no options</p>
          )}
          {options.map((o, i) => {
            const checked = values.includes(o.value);
            // Options arrive pre-grouped, so a header is the first row of each
            // run of same-group options.
            const header = o.group && o.group !== options[i - 1]?.group ? o.group : null;
            return (
              <Fragment key={o.value}>
                {header && (
                  <div className="px-2.5 pt-2 pb-1 font-mono text-[10px] tracking-wider text-muted uppercase">{header}</div>
                )}
                <button
                  type="button"
                  role="option"
                  aria-selected={checked}
                  className={`${OPTION_ROW} ${checked ? 'text-foreground' : ''}`}
                  onClick={() => toggle(o.value)}
                >
                  <span
                    className={`grid h-3.5 w-3.5 shrink-0 place-items-center rounded-[3px] border transition-colors ${
                      checked ? 'border-foreground/80 bg-foreground/80 text-background' : 'border-border text-transparent'
                    }`}
                  >
                    <Check size={10} strokeWidth={3.5} />
                  </span>
                  <span className="min-w-0 flex-1 truncate">{o.label}</span>
                </button>
              </Fragment>
            );
          })}
        </div>,
        document.body,
      )}
    </div>
  );
}
