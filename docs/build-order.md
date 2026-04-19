# Build Order

See DESIGN.md §Build order for the authoritative sequence and rationale.

## Main line (architectural risk)

1. `family-enum` — 6561 families with derived scalar features; serialized artifact.
2. `family-graph` — coarse potential adjacency; edge taxonomy + v0 weights; serialized.
3. `crude-layout` — R³ placement via depletion/diff/phase rules + edge attraction.
4. `viewer` — winit + wgpu static viewer; free-camera navigation; **first hard constraint test**.

## Side line (exactness kernel, independent)

A. `placement-codec` — combinadic rank/unrank over non-pawn piece placements.
B. `legality-predicate` — king adjacency, check/castling/ep consistency.

Per-step build notes will be added here as each crate reaches completion.
