# Artifact Formats

All artifacts are flat binary files produced offline and loaded at startup.
Serialization uses `bincode` (v1 API). See DESIGN.md §Conventions for rationale
(no database; static tables are flat lookup artifacts).

## family-enum table

File: `family_table.bin`  
Type: `Vec<family_enum::FamilyRecord>` (6561 entries, stored in `key.index()` order).  
Fields per record: `FamilyKey` (wnp_band, bnp_band, wp, bp) + `FamilyFeatures`
(wnp_center, bnp_center, total_material, material_diff, phase_estimate, depletion,
feature_span).

## family-graph adjacency

File: `family_graph.bin`  
Type: `petgraph::Graph<FamilyKey, EdgeMeta, Undirected>`.  
One structural edge per unordered family pair; edge metadata carries the
aggregated multiset of contributing `EdgeType` values and the v0 layout weight
(max of contributing type weights).

## crude-layout table

File: `layout_table.bin`  
Type: `crude_layout::LayoutTable` — parallel to the family table, one
`FamilyLayout` entry per family index.  
Fields per entry: `center: glam::Vec3`, `orientation: glam::Mat3`,
`extent_budget: ExtentBudget` (he, hn, hr).
