# Component recipes

Build these in order. Use them verbatim — the whole point is that there is one
grammar per job. React + Tailwind + `motion/react` shown; the classes are the
contract, the framework is not.

---

## 1. Tooltip — replaces every native `title`

Portalled to `<body>` and positioned with fixed coords. Native `title` must not
survive anywhere in the app.

```tsx
// GAP = 8px between trigger and tooltip
createPortal(
  <div
    className="rounded border border-border bg-surface px-1.5 py-0.5
               text-[9px] whitespace-nowrap text-muted"
    style={{ position: "fixed", top: y, left: x, zIndex: 9999 }}
  >
    {label}
  </div>,
  document.body
);
```

Rules: `text-[9px]`, `text-muted`, `rounded` (not xl), 1px border, dismiss on
mouseleave / blur / Escape. Never truncate silently — clamp to the viewport.

---

## 2. Select — the dropdown

```tsx
// trigger: standard control shell (see §6)
// panel:
<motion.div
  className="overflow-hidden rounded-xl border border-border bg-surface shadow-lg"
  initial={{ opacity: 0, y: -4 }}
  animate={{ opacity: 1, y: 0 }}
  transition={{ duration: 0.12 }}
  style={{ position: "fixed", top, left, width, zIndex: 9999 }}
>
  <div className="p-1.5">
    <button
      className="w-full rounded-lg px-2.5 py-1.5 text-left text-xs font-mono
                 text-muted hover:bg-border/30 hover:text-foreground"
    >
      {option.label}
    </button>
  </div>
</motion.div>
```

Chevron rotates `180deg` over `0.15s`. Empty state: lowercase `no options`,
`px-2.5 py-1.5 text-xs text-muted`. Panel portals to `<body>`, clamped to the
viewport, opens upward when space is short, closes on outside mousedown +
Escape.

---

## 3. ContextMenu / OverflowMenu — same shell, different trigger

```tsx
<motion.div
  className="min-w-[168px] overflow-hidden rounded-xl border border-border
             bg-surface p-1.5 shadow-lg"
  initial={{ opacity: 0, scale: 0.98 }}
  animate={{ opacity: 1, scale: 1 }}
  transition={{ duration: 0.1 }}
>
  <button className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5
                     text-sm text-muted hover:bg-border/30 hover:text-foreground">
    {icon} {label}
  </button>

  <button className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5
                     text-sm text-red-400 hover:bg-red-500/10">
    {icon} delete
  </button>
</motion.div>
```

`OverflowMenu` is the same panel opened from a `…` trigger; open upward when
`rect.top > innerHeight * 0.6`. Danger rows are the only place red appears —
never use the accent for destructive actions.

---

## 4. Overlay / Modal — one shell, shared by every overlay

If two overlays in the app differ in chrome, that is a bug.

```tsx
<>
  <div
    className="fixed inset-0 z-[9998]"
    style={{ background: "rgba(0,0,0,0.6)" }}
    onMouseDown={close}
  />
  <motion.div
    className="fixed left-1/2 top-1/2 z-[9999] flex -translate-x-1/2
               -translate-y-1/2 flex-col overflow-hidden rounded-xl
               border border-border"
    style={{
      background: "var(--color-surface)",
      width: "min(92vw, 960px)",
      height: "min(88vh, 680px)",
    }}
    initial={{ opacity: 0, scale: 0.99 }}
    animate={{ opacity: 1, scale: 1 }}
    transition={{ duration: 0.14 }}
  >
    <header className="flex items-center justify-between border-b border-border
                       px-4 py-2">
      <span className="text-sm font-medium text-foreground">settings</span>
      <Tooltip label="close">
        <button className="rounded-lg p-1.5 text-muted hover:bg-background
                           hover:text-foreground transition-colors">
          <X size={14} />
        </button>
      </Tooltip>
    </header>

    <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden">
      {children}
    </div>
  </motion.div>
</>
```

Escape closes. Dim backdrop is `rgba(0,0,0,0.6)` exactly. No shadow on the
backdrop; the panel may carry `shadow-lg`.

---

## 5. Tabs

**Top file tab** — dense, mono, rect-ish:

```tsx
// container: h-[28px], maxWidth 160, text-xs font-mono
// active:   bg-surface border border-border text-foreground
// inactive: text-muted hover:text-foreground hover:bg-muted/40
// label:    flex-1 min-w-0 truncate
```

An active tab **merges into the panel below** — same `bg-background`, no
separating underline, no bright border. Selection reads via the fill + a 5×5
`rounded-full` **type dot** colored to the content type (this dot is the one
sanctioned micro-use of extra hue, and only when a type distinction is real).

**Signature curved tab** (optional flourish — the one expressive element):
an SVG path with a corner radius (~10) and an outward flare (~8) per side,
`fill = isActive ? var(--color-background) : var(--color-surface)`,
`stroke var(--border)` at `strokeWidth 0.5`, width animating `150ms ease-in-out`.

---

## 6. Control / Button — the reference control

```tsx
<motion.button
  whileTap={{ scale: 0.88 }}
  transition={{ type: "spring", stiffness: 500, damping: 20 }}
  className="focus-ring h-10 rounded-xl border border-border bg-surface px-3
             font-mono text-[11px] lowercase text-muted shadow-lg
             transition-colors hover:bg-background hover:text-foreground"
>
  {label}
</motion.button>
```

- Icon-only variant: `w-10 h-10 rounded-lg` (same shell).
- Hover scale for bar actions: `1.05` labelled / `1.16` icon-only.
- Primary CTA (sparingly, e.g. onboarding): `bg-accent/80 hover:bg-accent
  text-accent-foreground font-mono text-sm tracking-wide duration-150`.
- **Never** point a button's default variant at the accent.

**Icon button** (toolbars, close, toggles):

```tsx
className="rounded-lg p-1.5 text-muted transition-colors hover:bg-background
           hover:text-foreground"
// toggled/active adds: text-primary bg-background
```

---

## 7. Inputs

```tsx
// inline (sidebar create):
className="rounded border border-border bg-background px-1 py-0.5
           text-[11px] font-mono text-foreground outline-none
           focus:border-foreground/40"

// standard:
className="rounded border border-border bg-background px-2 py-1 text-xs
           text-foreground outline-none placeholder:text-muted/50
           focus:border-foreground/40"
```

Focus is a border shift to `foreground/40`, never an accent glow.

---

## 8. Card / panel

```tsx
<div className="rounded-xl border border-border bg-surface px-4 py-3
                transition-colors hover:bg-background">
```

Panels are flat regions (`bg-surface` / `bg-background`) divided by
`border-b` / `border-r` — they do not get card chrome. Cards are for discrete
repeated items.

---

## 9. Toast / notification

```tsx
className="rounded-lg border border-border bg-background text-xs font-mono"
style={{ position: "fixed", top: 56, right: 16, zIndex: 9999,
         width: "clamp(240px, 20vw, 320px)" }}
// error variant: bg-red-500
// enter/exit: opacity + y(±12) over 0.25s easeOut
```

---

## 10. Rail / sidebar chrome

- Icon rail: `52px` wide, buttons `40×40 rounded-xl`; active = `background:
  var(--color-surface)` and stroke weight bumps `1.5 → 2`.
- Tree indentation is a manual rhythm: `paddingLeft: 8 + depth * 12`.
- Sidebar border: `1px` normally, `1.5px` only on the rail's outer edge.
- Resize separator: a `bg-border` line that brightens to `hover:bg-foreground/40`.

---

## Shared invariants (check every component against these)

- Every transient panel: portalled, `rounded-xl`, `border border-border`,
  `bg-surface`, `shadow-lg`, outside-mousedown + Escape, viewport-clamped.
- Every label in chrome is mono; every prose run is sans.
- Every icon is from one line-icon set. No emoji.
- Every color is a token. Zero hex literals in components.
- Every flexible label: `flex-1 min-w-0 truncate`. Every scroller:
  `overflow-x-hidden`.
