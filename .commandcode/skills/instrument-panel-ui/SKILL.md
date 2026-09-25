---
name: instrument-panel-ui
description: Build or restyle a UI in the pr0mptly "instrument panel" design philosophy — a near-black, border-first, mono-chromed dark interface with two surface planes, one reserved accent, dense chrome over airy content, and a single reused popover/overlay grammar. Use when the user asks for a dark UI, a technical/terminal/instrument-panel aesthetic, "make it look like pr0mptly", a dense desktop-tool look, or needs design tokens, component recipes, and motion specs for that style.
argument-hint: "[screen or component to design]"
---

# Instrument Panel UI

A dark-first design philosophy for technical tools. Chrome is dense and instrument-like; content is calm and full-bleed. Depth comes from two flat planes and hairline borders — not shadows. Hierarchy comes from brightness and surface fill — not color.

Read this before writing UI. Everything below is a constraint, not a suggestion.

## The thesis

**Two planes, one accent, borders over shadows, one grammar per pattern.** Exactly one chromatic decision exists in the whole interface (the accent), and it is reserved for confirm/emphasis. Everything else is neutral. Depth is a single step of lightness plus a border so subtle it nearly vanishes. Motion is short and tactile. Transient surfaces are all portalled and all built from the same recipe, because a second grammar for the same job is a defect.

## Non-negotiable rules

1. **Two surface planes, one step apart.**
   - `--background` = the darker **content plane**: workspace, editor, app body, and hover fills on chrome.
   - `--surface` = the lighter **chrome plane**: sidebar, top bar, active tabs, cards, popovers, modals, tooltips.
   - Do not add a third plane. Do not build elevation from a shadow ramp.
2. **Borders are the structure — and near-invisible on dark.** Target ~8–10% lightness above the surface (`#2a2a2a` on `#202020`). A border that "reads as a bright rule" is wrong. Never bright borders.
3. **Exactly one accent, reserved.** Accent is for confirm/emphasis affordances and the focus ring only. Never for selection, never for hover, never as per-type colors or badges. Selection is carried by neutral foreground + surface fill + a small type dot.
4. **Neutral hierarchy.** Secondary text is `text-muted`, primary is `text-foreground`. Hover brightens text or adds `hover:bg-background`; it never adds color. Disabled is opacity, not grey.
5. **Two-typeface grammar.** Mono for *all chrome*: labels, buttons, menus, counts, tabs, tooltips, metadata. Sans for *content* (prose, documents). Never mono prose; never sans controls.
6. **Casing rule.** Section/group labels: `uppercase tracking-wider text-[10px] text-muted`. Action labels: lowercase (`settings`, `copy`, `export`).
7. **Density split.** Chrome is dense — 9–12px type, 2–8px gaps, 1px borders, small integer spacing (`gap-1`, `px-2 py-1`). Content is airy — `line-height: 1.65`, generous block margins. Never airy chrome or cramped content.
8. **Motion is short and tactile.** 100–140ms fades for transient UI, springs (`stiffness 500 / damping 20`) for tap feedback, one reused easing curve `[0.2, 0, 0, 1]`. Nothing over 250ms except brand moments.
9. **One grammar per pattern.** Every menu, dropdown, context menu, tooltip, and floating panel uses the same recipe. Two interaction grammars for one job is a bug to fix, not a style choice.
10. **Portal everything transient** to `<body>`. No z-index can escape an ancestor's `overflow` clip — only leaving the tree can.
11. **No native controls, no emoji, no raw hex in components.** Restyle `<select>`, `<input type="range">`, and native `title` tooltips behind `Select`/`Slider`/`Tooltip` primitives. Icons are one line-icon set (e.g. Lucide). Components consume tokens, never hardcoded hex.
12. **Full-width, full-height, internally scrolling.** No centered measure caps on app content. Shell is `100vh; overflow: hidden`; panes are `flex-1 min-h-0` and scroll internally; chrome bars sit outside the scroller (`shrink-0`). **Zero horizontal overflow, ever** — `overflow-x-hidden` on every scroller, `flex-1 min-w-0 truncate` on every flexible label.
13. **Chrome recedes; content leads.** No section headers restating the view, no title/subtitle repeating the active tab. Open straight into content. Brand moments (boot splash, screen transitions, one signature element) are the only places allowed to be expressive.

## Procedure

1. **Tokens first.** Copy `references/tokens.css` into the project's global stylesheet and set the single accent to the project's brand color. This is the only place color is chosen.
2. **Expose utilities.** Map tokens into the utility layer (`@theme inline` for Tailwind v4; `:root` vars otherwise) so `bg-surface`, `bg-background`, `text-muted`, `border-border`, `rounded-xl` all resolve.
3. **Global base rules.** Default border color on `*`, 6px custom scrollbars, `.focus-ring`, range-slider restyle.
4. **Build primitives in order.** `Tooltip` → `Select` → `ContextMenu`/`OverflowMenu` → `Button`/`IconButton` → `Input` → `Card` → overlay shell (`Modal`) → toast. Use `references/component-recipes.md` verbatim; do not invent extra variants.
5. **Apply motion** from `references/motion.md`.
6. **Run the checklist** below before declaring done.

## Token facts

Copy `references/tokens.css` whole. Key points:

- Master radius `--radius: 0.75rem`; `--radius-xl = --radius + 4px`.
- Radii by role: cards/popovers/modals/tooltips = `rounded-xl`; buttons/rail/tab actions = `rounded-lg`; inline inputs/chips = `rounded`.
- Dark planes: `--background #191919`, `--surface #202020`, `--border #2a2a2a`, `--muted #8a8a8a`, `--foreground #f0efed`.
- `--color-primary` is **deliberately neutral** (`= foreground`). Never point it at the accent.
- Global default border: `@layer base { * { @apply border-border } }`.

## The two shells (memorize these)

**Transient / popover grammar** — every menu, dropdown, context menu, tooltip, floating panel:
```
portalled to <body> · viewport-clamped · rounded-xl · border border-border ·
bg-surface · shadow-lg · fade opacity 0 → 1 over 0.10–0.12s ·
dismiss on outside mousedown + Escape · opens upward when low on screen
```

**Overlay / modal shell** — shared by every full overlay, no exceptions:
```
backdrop: fixed inset-0, rgba(0,0,0,0.6)
panel:    rounded-xl border border-border, background var(--color-surface)
header:   border-b border-border px-4 py-2, lowercase text-sm font-medium title + X icon button
motion:   opacity 0 → 1 + scale 0.99 → 1 over 0.14s; Escape closes
```

Full recipes with exact classes: `references/component-recipes.md`.

## Review checklist

Reject the work if any of these hold:

- [ ] More than one accent hue, or accent used for selection/hover.
- [ ] A bright border, a shadow used for depth on a non-floating element, or a third surface plane.
- [ ] Sans-serif chrome, or mono-serif prose.
- [ ] A surviving native `<select>`, range input, or `title` tooltip.
- [ ] Selection conveyed by color instead of brightness + surface fill.
- [ ] Any horizontal scrollbar, or a flexible label without `min-w-0 truncate`.
- [ ] A transient panel that is neither portalled nor built from the shared shell.
- [ ] Motion slower than 250ms outside a brand moment, or more than one easing curve.
- [ ] Raw hex in a component, or an emoji used as an icon.
- [ ] Content constrained to a centered measure cap, or a scroller that isn't internal to a full-height pane.

## Adapting

The philosophy is dark-first but not dark-only — the light tokens in `references/tokens.css` follow the same two-plane + subtle-border logic. Keep the accent identical across themes. When a project already has a design system, apply the *rules* (two planes, neutral hierarchy, one accent, one grammar, density split) rather than overwriting its tokens.
