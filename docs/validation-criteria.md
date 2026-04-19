# Validation Criteria

See DESIGN.md §Validation criteria for the authoritative list.

## Family layer acceptance criteria

- [ ] Family count = 6561.
- [ ] Per-family exact-composition count not catastrophically skewed (measure post-build).
- [ ] Single-capture transitions usually local in family space (measure post-layout).
- [ ] Within-family exact compositions admit meaningful secondary layout.
- [ ] Coarse map interpretable by inspection (post-build check).

## Hard constraint (overarching)

Free-camera motion through 6561 static primitives: no stutter, no pop-in/pop-out,
legible at all focus/zoom combinations. If this fails, nothing downstream matters.
