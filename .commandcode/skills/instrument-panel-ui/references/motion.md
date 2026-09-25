# Motion

Short and tactile. One easing curve, a small set of durations, springs only for
direct manipulation feedback. If a transition needs more than 250ms it is a
brand moment, not UI.

## Durations

| Surface | Animation | Duration |
|---|---|---|
| Tooltip | opacity | 0.10s |
| Context / overflow menu | opacity + scale `0.98 → 1` | 0.10s |
| Dropdown / select | opacity + y `-4 → 0` | 0.12s |
| Chevron rotate (select) | transform | 0.15s |
| Overlay / modal | opacity + scale `0.99 → 1` | 0.14s |
| View crossfade | opacity | 0.12s |
| Panel swap | opacity + y `6 → 0` | 0.15s |
| Toast / notice in-out | opacity + y `±12` | 0.25s easeOut |
| Hover fills / text | `transition-colors` | default (≈0.15s) |
| Slider thumb | transform | 0.12s |

## Easing

One authored curve, reused everywhere:

```
const EASE = [0.2, 0, 0, 1];
```

Toasts use `easeOut`. Everything else uses `EASE` or the default color
transition. Do not introduce a second named easing.

## Springs — the tactile signature

Direct-manipulation feedback only (taps, flips, reveals):

```tsx
// tap
whileTap={{ scale: 0.88 }}
transition={{ type: "spring", stiffness: 500, damping: 20 }}

// card hover / tap
whileHover={{ scale: 1.02 }}
whileTap={{ scale: 0.97 }}
transition={{ type: "spring", stiffness: 500, damping: 24 }}

// floating bar reveal
transition={{ type: "spring", stiffness: 420, damping: 30 }}
// hide: duration 0.14, ease EASE, from { scale: 0.9, y: 28, rotate: -12 },
// transformOrigin: "bottom right"
```

Rule of thumb: springs for scale, tweens for opacity/position. Nothing
bouncy beyond the values above — this reads as precision, not playfulness.

## Reduced motion

Gate spring/hover micro-interactions behind a `prefers-reduced-motion` check
(`usePrefersReducedMotion` or a media query). Opacity fades are acceptable;
scale/translate springs are not.

## Brand moments (the only slow motion)

Boot splash, vault/screen transitions, and the one signature flourish may use
longer timings — e.g. logo fade `0.4s` with `EASE`, a `2.4s` bloom pulse loop,
a `1.3s` indeterminate sweep. Keep these to 2–3 places in the whole app.
