# Non-Commitments

See DESIGN.md §Deferred for the full list of what is explicitly out of scope for v0.

Summary of deferred items:
- Engine oracle (Stockfish UCI) for north/south eval precision.
- Syzygy tablebase integration for endgame attractor structure.
- Exact-composition sub-chart within families.
- Refined edge weights (support factor, band-jump magnitude).
- Deeper hierarchy levels: pawn basin, stm, castling, ep partitioning rules.
- Per-pixel descent rendering.
- Visit fiber (threefold repetition stacked sub-cells).
- Persistence beyond static layout table.

Nothing in this list will be designed or implemented before the first executable
milestone (family enumeration → coarse graph → crude layout → static viewer) is
validated against the hard camera-motion constraint.
