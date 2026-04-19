# chessviz2 — Design Surface

This document captures the locked v0 design surface for the chess manifold
visualization project. Heuristics and deferred areas are explicitly
labeled as such. The decisions here are load-bearing and should not
drift without deliberate reconsideration; the document is not a claim
that every question has been answered, but the committed surface is
enough to build the first executable milestone without further upstream
design work.

## What this project is

A 3D visualization of the chess game space as a navigable manifold. The
camera floats freely through the manifold. An occupied leaf cell at the
currently realized hierarchy level corresponds to a specific state at
that level of refinement; a representative point (centroid) inside that
cell is used for rendering. An arbitrary point in R³ must first be
mapped by containment into the partition, and may land in empty space.
Most R³ points are not state-bearing. Coarse structure is legible at low
zoom (basin-like regions defined by material composition); fine
structure resolves progressively on zoom-in as deeper hierarchy levels
are realized.

The manifold is an *intricate 3D object* — it must support free camera
motion without stutter and without pop-in/pop-out, at any zoom and focus
combination. This is the hard constraint and the evaluation surface.

Rendering is a Rust + wgpu + winit native application. No web stack, no
Tauri (at least not initially).

## Bijection formulation

States are in bijection with **leaf cells** of a hierarchical partition
of R³, not with points. Cells have nonzero volume; containment is the
inverse operation. A representative point (centroid) within each cell is
used for rendering.

A point-based bijection between continuous R³ and the discrete state
space would be a category error: there are uncountably many points and
countably many states. The cell-based formulation is the correct one.
Most points in R³ are not state-bearing; they are either inside empty
cells or outside any currently-realized cell.

An occupied leaf cell at the currently realized hierarchy level
corresponds to a specific state at that level of refinement. At v0, only
the family layer is realized, so "occupied leaf cell" means "an occupied
family cell." As deeper levels come online, each will add its own
refinement of what a leaf is.

## Hierarchy (locked)

```
family
  → exact composition
    → pawn basin
      → stm
        → castling
          → ep
            → placement address
              → visit fiber (zoom-revealed)
```

Each level partitions the parent cell into sub-cells by a level-specific
rule. Cells at each level are indexed exactly; legality and visual
coherence are separate concerns (see below).

### Level semantics (what each level actually encodes)

- **family** — material composition binned by the band table (see below).
- **exact composition** — the exact non-pawn piece multiset per side
  (promotion-aware), fully determining piece counts for the placement
  codec.
- **pawn basin** — the pawn structure (which squares the pawns occupy),
  given the exact composition's pawn counts.
- **stm** — side to move ∈ {white, black}.
- **castling** — castling rights bitmask, a subset of {W-kingside,
  W-queenside, B-kingside, B-queenside}.
- **ep** — en-passant **availability**. This is *not* the raw FEN
  en-passant target square. It records whether an actual en-passant
  capture is available to the side to move, which is the distinction
  that matters for move generation and for repetition semantics. A
  position where a pawn double-stepped but no opposing pawn can capture
  en-passant is ep-unavailable, even if the FEN target field would
  name a square. Crates must encode ep as availability, not as raw FEN.
- **placement address** — exact combinadic rank of non-pawn piece
  placement on the free-square set, given all levels above.
- **visit fiber** — zoom-revealed multiplicity for repeated visits
  within the current reversible era. Not part of v0.

## Family key (locked)

```
Family = (WNP_band, BNP_band, WP, BP)
```

where:

- `WNP_band` / `BNP_band` — non-pawn material per side, banded into nine
  ranges. Non-pawn piece values are Q=9, R=5, B=3, N=3. The theoretical
  maximum per side is 31 (Q + 2R + 2B + 2N = 9+10+6+6); there is no
  "27+" open tail because non-pawn material cannot exceed 31.
- `WP` / `BP` — pawn count per side, 0..8 exactly (no binning).

Family count: 9 × 9 × 9 × 9 = **6561**.

### Band table (canonical, used by all crates)

| Band index | Range (inclusive) | Center | Span |
|------------|-------------------|--------|------|
| 0          | 0                 | 0.0    | 0    |
| 1          | 1–3               | 2.0    | 2    |
| 2          | 4–5               | 4.5    | 1    |
| 3          | 6–8               | 7.0    | 2    |
| 4          | 9–11              | 10.0   | 2    |
| 5          | 12–15             | 13.5   | 3    |
| 6          | 16–20             | 18.0   | 4    |
| 7          | 21–26             | 23.5   | 5    |
| 8          | 27–31             | 29.0   | 4    |

These are canonical. All crates that need a band center or span must
read them from a single shared constant; crates must not independently
redefine them. The values are chosen so each band captures a natural
"material state" (single minor, two minors or rook, etc.) — the exact
numbers can be revised, but they must be revised in one place.

Why this key: bounded and enumerable, semantically legible (matches how
chess is discussed at the material-composition level), compatible with
exact composition as the next level down. It is good enough to commit
without further refinement. Piece-shape ambiguity at equal total material
(queen vs rook+minor, etc.) is a known weakness — to be handled by a
secondary in-family chart later if it proves visually important, not by
changing the family key.

## Family artifact

```
T_family[family] = (center, orientation, extent_budget, feature_span)
```

- `center` — R³ centroid
- `orientation` — local frame / basis
- `extent_budget` — `(hE, hN, hR)` half-extents per axis (east, north/south, radial)
- `feature_span` — within-band semantic width (e.g., band 21-26 has wider
  span than band 4-5; the family's local budget must know this)

Band centers are used for layout-relevant derived features (material_diff,
phase estimate, total material). Feature span informs anisotropic extent.

## Graph semantics (locked, named honestly)

The family transition graph is **coarse potential adjacency**, not
certified move existence.

> A family edge between F1 and F2 records that the family keys differ by
> a one-move material delta — some conceivable single-ply move with
> matching material-key effect could connect some position in F1 to some
> position in F2. This is a necessary condition for any actual
> move-connection, not a sufficient one.

The graph is permissive: some edges will correspond to no actually-reachable
move at any position in F1. That's acceptable. Tightening with
exact-composition witnesses is deferred.

### Edge taxonomy

Edge metadata must carry the captured piece class where applicable, so
that weights distinguishing minor vs major captures can be assigned
correctly. The sub-types below are the granularity the v0 weights need.

- **Type B-minor** — minor piece (B or N) captured, crossing a WNP band:
  reduces victim's WNP_band
- **Type B-major** — major piece (R or Q) captured, crossing a WNP band:
  reduces victim's WNP_band
- **Type C** — pawn captured by non-pawn: reduces victim's WP or BP by 1
- **Type D** — pawn-captures-pawn: reduces victim's WP or BP by 1
- **Type E** — non-capturing promotion: reduces promoter's pawn count by 1,
  increases promoter's WNP_band
- **Type F** — capturing promotion: combines E with B-minor, B-major, or C

Same-band captures (within-band WNP reduction) are NOT family edges —
they're within-family transitions at the exact-composition level.

Reversible moves never cross family boundaries (material composition
preserved).

### Edge weights (v0 constants only)

These are **layout heuristics for version 0**, not long-term bridge
semantics. They must be assignable from the edge taxonomy alone (i.e.,
using the captured piece class carried in edge metadata):

- Type D (pawn-captures-pawn): 0.35
- Type B-minor (minor capture, band-crossing): 0.30
- Type B-major (major capture, band-crossing): 0.10
- Type C (non-pawn captures pawn): 0.08
- Type E or F (promotion of any kind): 0.02

The real weight structure is known and deferred:
`weight = move_type_prior × family_support_factor × band_jump_factor`.

### Storage (simple graph with aggregated metadata)

One structural edge per unordered family pair. The graph is a simple
graph, not a multigraph. When edge enumeration would produce multiple
reasons for the same family pair (e.g., the family keys differ in a way
satisfiable by both a Type C and a Type D transition, or by multiple
non-pawn capture types), those reasons are **aggregated onto the single
edge** as a list of contributing coarse-delta reasons (a multiset of
edge types).

- **Structural adjacency** — undirected, one edge per family pair, drives
  layout attraction.
- **Interpretive edge metadata** — per-edge: the multiset of contributing
  edge types, direction of material change per type, band-jump magnitude
  per type. Used for rendering and future weight refinement.

**v0 layout weight from aggregated metadata:** the layout weight for an
edge is the **maximum** of the v0 heuristic weights across the
contributing edge types. (Rationale: max rather than sum keeps the
weight interpretable as "the strongest kind of transition this edge
supports" and avoids double-counting when several taxonomy types happen
to apply to the same coarse delta.) This choice is explicit and
documented here so no crate invents a different aggregation.

## East-at-family-scale: compositional depletion only

East coordinate for a family is derived directly from the family key as a
monotone depletion measure from starting material. Starting totals are
31 points of non-pawn material per side (Q=9 + R+R=10 + B+B=6 + N+N=6)
and 8 pawns per side, so the maximum depletion sum is 31+31+8+8 = 78.

```
depletion(F) = g( (31 - WNP_center(F))
                + (31 - BNP_center(F))
                + (8  - WP(F))
                + (8  - BP(F)) )
```

where `g` is a monotone (optionally normalized) scalar map into the east
axis range. `WNP_center` and `BNP_center` are the band-center piece-value
sums; `WP` and `BP` are exact pawn counts. All four terms are in
comparable "point" units (pawn = 1 for this purpose), so the sum is
dimensionally consistent.

**Not** BFS distance in the graph. Reason: many irreversible moves don't
cross family boundaries (pawn pushes, castling-right loss, ep state), so
graph distance would claim more eastward structure than the family layer
can deliver.

Full-sense irreversibility is a sum across levels. Family contributes
compositional depletion. Deeper levels contribute pawn-history, castling-
history, ep-state, and so on. Each level measures the irreversibility
visible at that level.

## Placement address (locked, side-line work)

Exact sparse combinadic rank over non-pawn piece placements, given fixed
`(exact_composition, pawn_basin, stm, castling, ep)`.

The key point: the codec needs the non-pawn piece multiset determined
exactly, which the *exact composition* level provides. Family alone does
not fix piece counts (the band centers are only approximations; the
family can contain many exact compositions with different piece
multisets). The pawn basin fixes where pawns actually sit, which is what
determines the free-square set `S_0` the codec operates over.

### Piece-family order (committed)

`WK, BK, WQ, BQ, WR, BR, WB, BB, WN, BN`

Kings first so king-position similarity aligns with rank-neighborhood.

### Mechanism

Mixed-radix combinadics over the dynamically-shrinking free-square set:

```
rank(placement) =
  encode(WK_square ∈ S_0)
  ∘ encode(BK_square ∈ S_1)
  ∘ encode(WQ_subset ∈ S_2)
  ∘ ...
```

`S_i` is the free-square set remaining after placing all prior piece
families (pawns pre-placed by pawn structure). For families with
identical pieces, rank the *subset* (combination), not the tuple —
handles indistinguishability naturally.

### Three jobs held separate

1. **Exact addressability** — combinadic, ignores legality
2. **Legality filtering** — predicate applied after decode. Specifically:
   - **King adjacency**: the two kings must not be on adjacent squares.
   - **Check consistency**: the side to move *may* be in check (that is a
     legal chess state — they have to get out of it). The side *not* to
     move must *not* be in check (if they were, the previous move would
     have been illegal). Do NOT reject all checked positions; that would
     destroy a large and legitimate part of the state space.
   - **Castling consistency**: any asserted castling right requires the
     relevant king and rook to be on their starting squares.
   - **En-passant consistency**: an ep-available state is legal only if
     it is consistent with a just-played double-step *and* with an
     actual en-passant capture being available to the side to move. A
     state that records ep-available but for which no opposing pawn can
     actually perform the en-passant capture is illegal. This follows
     from the ep-as-availability semantics defined in the hierarchy
     section: ep records whether the capture is available, not merely
     whether a double-step happened.
3. **Visual coherence** — optional secondary remap within a chart, not
   baked into the rank

King adjacency goes in the legality predicate, not in the rank scheme.
Uniform piece-family treatment in the codec; no special cases.

## Split rule principles

Each level's subdivision respects its extent budget per axis:

- **East** — advance by irreversibility/depletion at that level's scale
- **North/South** — outcome pull (material advantage, eval, tablebase
  result at endgame)
- **Radial** — ply/phase/temporal shelling

Split rules are per-level, not a single universal rule. The crude first
layout uses simple deterministic rules; refinement comes after measurement.

## Deferred (explicitly not part of v0)

- Engine oracle integration (Stockfish UCI subprocess for eval and move
  ordering) — needed for north/south precision at non-tablebase positions,
  stood up when crude-layout proves the architecture.
- Syzygy tablebase integration — needed for endgame attractor structure
  at the east edge, stood up with engine oracle.
- Exact-composition sub-chart within families — queenless/queen-bearing
  as the likely first secondary organizing rule.
- Refined edge weights (support factor, band-jump magnitude).
- Deeper hierarchy levels (pawn basin, stm, castling, ep partitioning
  rules).
- Per-pixel descent rendering — the real shader complexity. v0 renders
  families as sized glyphs with bridges; descent rendering comes after
  the coarse layout is validated.
- Visit fiber structure (threefold repetition as stacked sub-cells).
- Persistence beyond static layout table.

## Build order

### Main line (architectural risk)

1. **Family enumeration** — produce the 6561 family set with derived
   features (band centers, pawn counts, material_diff, phase estimate,
   total material, feature_span, depletion scalar). Serialize to disk.
2. **Family transition graph** — enumerate edges by rule from the taxonomy,
   attach interpretive metadata, assign v0 constant weights. Serialize
   as adjacency list.
3. **Crude layout** — east from depletion, north/south from material_diff,
   radial from phase/total, edge-driven attraction, local repulsion for
   non-overlap. Serialize R³ placements with extent parameters.
4. **Static viewer skeleton** — winit + wgpu, families as sized glyphs at
   computed positions, edges as metadata-colored bridges, free-camera
   navigation. **This is the first hard-constraint test rig.**

### Side line (exactness kernel, independent)

A. **Placement codec** — combinadic rank/unrank, roundtrip tests.

B. **Legality predicate** — king adjacency, check consistency (see the
   Three Jobs section above for the precise semantics — side to move may
   be in check, side not to move must not be), castling consistency, ep
   consistency.

## Validation criteria

A family scheme / layout is acceptable if:

- Family count in low thousands ✓ (6561)
- Per-family exact-composition count not catastrophically skewed
  (measure post-build)
- Single-capture transitions usually local in family space
  (measure post-layout)
- Within-family exact compositions admit meaningful secondary layout
  (design constraint on sub-chart)
- Coarse map interpretable by inspection (post-build check)

Plus the overarching hard constraint: smooth free-camera motion, no
stutter, no pop, legible at all focus/zoom combinations.

## Stack

- **Rust** throughout, Cargo workspace with per-component crates.
- **shakmaty** for position representation, move generation, Zobrist.
- **petgraph** for offline graph computation (material DAG, family graph).
- **nalgebra** or **glam** for R³ math. Prefer glam for wgpu compatibility.
- **wgpu** for rendering. WGSL shaders. No higher-level engine (not bevy,
  not rend3) — the per-pixel descent architecture doesn't fit scene-graph
  assumptions.
- **winit** for windowing and input.
- **rayon** for parallel offline work where relevant.
- **bincode** or **rkyv** for static artifact serialization.
- **shakmaty-syzygy** for tablebases (deferred).

No database. Static layout tables are flat lookup artifacts loaded at
startup. Session-scoped memoization is a cache, not authoritative state —
reconstructible from pure functions + tables.

## Workspace structure

```
chessviz2/
├── README.md
├── DESIGN.md                     (this file)
├── Cargo.toml                    (workspace root)
├── docs/
│   ├── build-order.md
│   ├── non-commitments.md
│   ├── validation-criteria.md
│   └── artifact-formats.md
├── crates/
│   ├── family-enum/              main line 1
│   ├── family-graph/             main line 2
│   ├── crude-layout/             main line 3
│   ├── viewer/                   main line 4
│   ├── placement-codec/          side line A
│   └── legality-predicate/       side line B
└── .gitignore
```

## Conventions / stance

- No database in the design. Flat tables and pure functions.
- The bijection is cell-based. Points are representatives, not carriers
  of state identity.
- Three jobs of rank/address: exact addressability, legality filtering,
  visual coherence. Kept separate, one mechanism each.
- "Permissive scaffold" at the family graph layer — named as potential
  adjacency, not certified. Tightening is a later refinement.
- East at any level means *the irreversibility visible at that level*.
  Full-sense east is a sum across levels.
- Edge weights at v0 are constants and explicitly labeled as layout
  heuristics, not long-term semantics.
- First hard-constraint test is the static viewer on the coarse family
  layout. If the camera can't fly through 6561 static primitives
  smoothly, nothing downstream matters.

## First executable milestone

```
family enumeration
  → coarse potential adjacency graph
    → crude layout
      → static viewer with free camera
```

When the coarse family artifact runs smoothly and reads as legible
space — with capture-adjacent families landing locally and the camera
meeting the hard constraint on 6561 static primitives — the
**family-layer architecture** is provisionally validated. This does
not validate the full manifold; deeper levels (exact composition,
pawn basin, stm, castling, ep, placement, visit fiber) each have their
own risks and will be validated in turn as they are built.