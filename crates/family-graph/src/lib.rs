use family_enum::{FamilyKey, FamilyRecord, BAND_TABLE};
use petgraph::graph::{Graph, NodeIndex};
use petgraph::Undirected;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Edge taxonomy from DESIGN.md §Edge taxonomy.
///
/// The distinction between B-minor and B-major determines which v0 weight
/// applies; F variants combine promotion with a capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeType {
    /// Minor piece (B or N) captured, crossing WNP band.
    BMinor,
    /// Major piece (R or Q) captured, crossing WNP band.
    BMajor,
    /// Non-pawn captures pawn (pawn count decreases, no band crossing).
    C,
    /// Pawn captures pawn.
    D,
    /// Non-capturing promotion (pawn count −1, WNP band increases).
    E,
    /// Capturing promotion combining E with BMinor.
    FWithBMinor,
    /// Capturing promotion combining E with BMajor.
    FWithBMajor,
    /// Capturing promotion combining E with C.
    FWithC,
}

impl EdgeType {
    /// V0 layout heuristic weight. See DESIGN.md §Edge weights.
    pub fn v0_weight(self) -> f32 {
        match self {
            EdgeType::D => 0.35,
            EdgeType::BMinor => 0.30,
            EdgeType::BMajor => 0.10,
            EdgeType::C => 0.08,
            EdgeType::E | EdgeType::FWithBMinor | EdgeType::FWithBMajor | EdgeType::FWithC => 0.02,
        }
    }
}

/// Aggregated metadata on one undirected family edge.
///
/// Multiple coarse-delta reasons for the same family pair are aggregated
/// here rather than creating a multigraph. Layout weight = max of
/// contributing type weights (see DESIGN.md §Storage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeMeta {
    /// All edge types that contribute to this family pair transition.
    pub edge_types: Vec<EdgeType>,
    /// v0 layout weight = max of edge_types weights.
    pub layout_weight: f32,
}

impl EdgeMeta {
    /// Build `EdgeMeta` from a non-empty list of contributing types.
    pub fn from_types(types: Vec<EdgeType>) -> Self {
        let layout_weight = types
            .iter()
            .map(|t| t.v0_weight())
            .fold(f32::NEG_INFINITY, f32::max);
        EdgeMeta { edge_types: types, layout_weight }
    }
}

/// Undirected family transition graph.
/// Node weight: `FamilyKey`. Edge weight: `EdgeMeta`.
pub type FamilyGraph = Graph<FamilyKey, EdgeMeta, Undirected>;

// ── Band reachability helpers ─────────────────────────────────────────────────

/// True if removing `pv` points from some value in band `from` can land in band `to`.
/// Uses the permissive interpretation: only requires *some* value in the band to work.
fn can_remove_piece(from: usize, to: usize, pv: u8) -> bool {
    let bf = &BAND_TABLE[from];
    let bt = &BAND_TABLE[to];
    if bf.hi < pv {
        return false;
    }
    // v in [max(bf.lo, pv), bf.hi], so v - pv in [bf.lo.sat_sub(pv), bf.hi - pv]
    let after_lo = bf.lo.saturating_sub(pv);
    let after_hi = bf.hi - pv;
    after_lo <= bt.hi && bt.lo <= after_hi
}

/// True if adding `pv` points to some value in band `from` can land in band `to`.
/// Capped at 31 (non-pawn material ceiling).
fn can_add_piece(from: usize, to: usize, pv: u8) -> bool {
    let bf = &BAND_TABLE[from];
    let bt = &BAND_TABLE[to];
    let after_lo = bf.lo + pv;
    let after_hi = (bf.hi + pv).min(31);
    if after_lo > 31 {
        return false;
    }
    after_lo <= bt.hi && bt.lo <= after_hi
}

/// True if a minor-piece capture (value 3) crossing from band `from` to `to` is possible.
/// Requires `to < from` (same-band captures are within-family, not graph edges).
fn can_remove_minor(from: usize, to: usize) -> bool {
    to < from && can_remove_piece(from, to, 3)
}

/// True if a major-piece capture (R=5 or Q=9) crossing from band `from` to `to` is possible.
fn can_remove_major(from: usize, to: usize) -> bool {
    to < from && (can_remove_piece(from, to, 5) || can_remove_piece(from, to, 9))
}

/// True if promoting to any piece (N/B=3, R=5, Q=9) from band `from` can reach band `to`.
fn can_promote_to(from: usize, to: usize) -> bool {
    to > from
        && (can_add_piece(from, to, 3)
            || can_add_piece(from, to, 5)
            || can_add_piece(from, to, 9))
}

// ── Graph construction ────────────────────────────────────────────────────────

/// Enumerate all family edges by the coarse-delta rules and build the graph.
/// See DESIGN.md §Graph semantics and §Edge taxonomy.
pub fn build_graph(families: &[FamilyRecord]) -> FamilyGraph {
    // Accumulate edge types per unordered pair (min_idx, max_idx).
    let mut edge_map: HashMap<(usize, usize), Vec<EdgeType>> = HashMap::new();

    for rec in families {
        let k = rec.key;
        let idx = k.index();
        let w = k.wnp_band as usize;
        let b = k.bnp_band as usize;
        let wp = k.wp;
        let bp = k.bp;

        let mut push = |idx2: usize, et: EdgeType| {
            debug_assert_ne!(idx, idx2);
            let key = if idx < idx2 { (idx, idx2) } else { (idx2, idx) };
            edge_map.entry(key).or_default().push(et);
        };

        // BMinor / BMajor: white loses a non-pawn piece (victim = white)
        for w2 in 0..w {
            let idx2 = FamilyKey { wnp_band: w2 as u8, bnp_band: b as u8, wp, bp }.index();
            if can_remove_minor(w, w2) {
                push(idx2, EdgeType::BMinor);
            }
            if can_remove_major(w, w2) {
                push(idx2, EdgeType::BMajor);
            }
        }

        // BMinor / BMajor: black loses a non-pawn piece (victim = black)
        for b2 in 0..b {
            let idx2 = FamilyKey { wnp_band: w as u8, bnp_band: b2 as u8, wp, bp }.index();
            if can_remove_minor(b, b2) {
                push(idx2, EdgeType::BMinor);
            }
            if can_remove_major(b, b2) {
                push(idx2, EdgeType::BMajor);
            }
        }

        // Type C: non-pawn captures pawn
        // white non-pawn captures black's pawn
        if bp > 0 && w > 0 {
            let idx2 =
                FamilyKey { wnp_band: w as u8, bnp_band: b as u8, wp, bp: bp - 1 }.index();
            push(idx2, EdgeType::C);
        }
        // black non-pawn captures white's pawn
        if wp > 0 && b > 0 {
            let idx2 =
                FamilyKey { wnp_band: w as u8, bnp_band: b as u8, wp: wp - 1, bp }.index();
            push(idx2, EdgeType::C);
        }

        // Type D: pawn captures pawn (capturer's pawn count unchanged)
        // white pawn captures black's pawn
        if wp > 0 && bp > 0 {
            let idx2 =
                FamilyKey { wnp_band: w as u8, bnp_band: b as u8, wp, bp: bp - 1 }.index();
            push(idx2, EdgeType::D);
            // black pawn captures white's pawn
            let idx3 =
                FamilyKey { wnp_band: w as u8, bnp_band: b as u8, wp: wp - 1, bp }.index();
            push(idx3, EdgeType::D);
        }

        // Type E: non-capturing promotion
        // white promotes
        if wp > 0 {
            for w2 in (w + 1)..9 {
                if can_promote_to(w, w2) {
                    let idx2 = FamilyKey {
                        wnp_band: w2 as u8,
                        bnp_band: b as u8,
                        wp: wp - 1,
                        bp,
                    }
                    .index();
                    push(idx2, EdgeType::E);
                }
            }
        }
        // black promotes
        if bp > 0 {
            for b2 in (b + 1)..9 {
                if can_promote_to(b, b2) {
                    let idx2 = FamilyKey {
                        wnp_band: w as u8,
                        bnp_band: b2 as u8,
                        wp,
                        bp: bp - 1,
                    }
                    .index();
                    push(idx2, EdgeType::E);
                }
            }
        }

        // Type F: capturing promotion
        // white promotes and captures a black piece
        if wp > 0 {
            for w2 in (w + 1)..9 {
                if can_promote_to(w, w2) {
                    for b2 in 0..b {
                        if can_remove_minor(b, b2) {
                            let idx2 = FamilyKey {
                                wnp_band: w2 as u8,
                                bnp_band: b2 as u8,
                                wp: wp - 1,
                                bp,
                            }
                            .index();
                            push(idx2, EdgeType::FWithBMinor);
                        }
                        if can_remove_major(b, b2) {
                            let idx2 = FamilyKey {
                                wnp_band: w2 as u8,
                                bnp_band: b2 as u8,
                                wp: wp - 1,
                                bp,
                            }
                            .index();
                            push(idx2, EdgeType::FWithBMajor);
                        }
                    }
                    if bp > 0 {
                        let idx2 = FamilyKey {
                            wnp_band: w2 as u8,
                            bnp_band: b as u8,
                            wp: wp - 1,
                            bp: bp - 1,
                        }
                        .index();
                        push(idx2, EdgeType::FWithC);
                    }
                }
            }
        }
        // black promotes and captures a white piece
        if bp > 0 {
            for b2 in (b + 1)..9 {
                if can_promote_to(b, b2) {
                    for w2 in 0..w {
                        if can_remove_minor(w, w2) {
                            let idx2 = FamilyKey {
                                wnp_band: w2 as u8,
                                bnp_band: b2 as u8,
                                wp,
                                bp: bp - 1,
                            }
                            .index();
                            push(idx2, EdgeType::FWithBMinor);
                        }
                        if can_remove_major(w, w2) {
                            let idx2 = FamilyKey {
                                wnp_band: w2 as u8,
                                bnp_band: b2 as u8,
                                wp,
                                bp: bp - 1,
                            }
                            .index();
                            push(idx2, EdgeType::FWithBMajor);
                        }
                    }
                    if wp > 0 {
                        let idx2 = FamilyKey {
                            wnp_band: w as u8,
                            bnp_band: b2 as u8,
                            wp: wp - 1,
                            bp: bp - 1,
                        }
                        .index();
                        push(idx2, EdgeType::FWithC);
                    }
                }
            }
        }
    }

    let mut graph: FamilyGraph = Graph::new_undirected();

    // Add all 6561 nodes in index order so NodeIndex::new(i) == families[i].
    let node_indices: Vec<NodeIndex> =
        families.iter().map(|rec| graph.add_node(rec.key)).collect();

    for ((idx1, idx2), types) in edge_map {
        graph.add_edge(node_indices[idx1], node_indices[idx2], EdgeMeta::from_types(types));
    }

    graph
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use family_enum::{build_table, FamilyKey};

    fn build_test_graph() -> FamilyGraph {
        let families = build_table();
        build_graph(&families)
    }

    fn node(key: FamilyKey) -> NodeIndex {
        NodeIndex::new(key.index())
    }

    fn get_edge<'a>(
        graph: &'a FamilyGraph,
        k1: FamilyKey,
        k2: FamilyKey,
    ) -> Option<&'a EdgeMeta> {
        graph
            .find_edge(node(k1), node(k2))
            .map(|e| graph.edge_weight(e).unwrap())
    }

    #[test]
    fn edge_count_plausibility() {
        let g = build_test_graph();
        let ec = g.edge_count();
        // 6561 nodes; rough estimate ~10k–500k edges
        assert!(ec > 10_000, "too few edges: {ec}");
        assert!(ec < 500_000, "suspiciously many edges: {ec}");
    }

    #[test]
    fn no_self_edges() {
        let g = build_test_graph();
        for e in g.edge_indices() {
            let (a, b) = g.edge_endpoints(e).unwrap();
            assert_ne!(a, b, "self-edge at node {a:?}");
        }
    }

    #[test]
    fn starting_family_has_bminor_edge() {
        // Starting family (8,8,8,8) should connect to (7,8,8,8):
        // removing a minor (value 3) from band 8 [27-31] → [24-28] overlaps band 7 [21-26].
        let g = build_test_graph();
        let start = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 };
        let neighbor = FamilyKey { wnp_band: 7, bnp_band: 8, wp: 8, bp: 8 };
        let meta = get_edge(&g, start, neighbor)
            .expect("expected BMinor edge from starting family to (7,8,8,8)");
        assert!(
            meta.edge_types.contains(&EdgeType::BMinor),
            "edge missing BMinor type; got {:?}",
            meta.edge_types
        );
    }

    #[test]
    fn pawn_capture_edge_has_c_and_d_with_correct_weight() {
        // (8,8,8,8) → (8,8,8,7): white captures black's pawn.
        // C applies (w=8 > 0 non-pawn), D applies (wp=8 > 0 pawn).
        // layout_weight = max(0.08, 0.35) = 0.35.
        let g = build_test_graph();
        let f1 = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 };
        let f2 = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 7 };
        let meta = get_edge(&g, f1, f2).expect("expected C/D edge");
        assert!(meta.edge_types.contains(&EdgeType::C), "missing C");
        assert!(meta.edge_types.contains(&EdgeType::D), "missing D");
        assert!(
            (meta.layout_weight - 0.35).abs() < 1e-6,
            "expected weight 0.35, got {}",
            meta.layout_weight
        );
    }

    #[test]
    fn weight_aggregation_uses_max() {
        // (5,5,4,4) → (4,5,4,4): white's NP band drops from 5 to 4.
        // Band 5 [12-15] minus 3 → [9-12] overlaps band 4 [9-11]: BMinor (0.30).
        // Band 5 [12-15] minus 5 → [7-10] overlaps band 4 [9-11]: BMajor (0.10).
        // layout_weight = max(0.30, 0.10) = 0.30.
        let g = build_test_graph();
        let f1 = FamilyKey { wnp_band: 5, bnp_band: 5, wp: 4, bp: 4 };
        let f2 = FamilyKey { wnp_band: 4, bnp_band: 5, wp: 4, bp: 4 };
        let meta = get_edge(&g, f1, f2).expect("expected BMinor+BMajor edge");
        assert!(meta.edge_types.contains(&EdgeType::BMinor), "missing BMinor");
        assert!(meta.edge_types.contains(&EdgeType::BMajor), "missing BMajor");
        assert!(
            (meta.layout_weight - 0.30).abs() < 1e-6,
            "expected weight 0.30, got {}",
            meta.layout_weight
        );
    }

    #[test]
    fn promotion_edge_is_type_e() {
        // (0,0,1,0) → (1,0,0,0): white promotes bare pawn to minor (value 3),
        // band 0 [0-0] + 3 = 3 → band 1 [1-3]. No captures possible (no enemy pieces/pawns).
        let g = build_test_graph();
        let f1 = FamilyKey { wnp_band: 0, bnp_band: 0, wp: 1, bp: 0 };
        let f2 = FamilyKey { wnp_band: 1, bnp_band: 0, wp: 0, bp: 0 };
        let meta = get_edge(&g, f1, f2).expect("expected E edge for bare promotion");
        assert!(meta.edge_types.contains(&EdgeType::E), "missing E type");
        assert!(
            (meta.layout_weight - 0.02).abs() < 1e-6,
            "expected weight 0.02, got {}",
            meta.layout_weight
        );
    }

    #[test]
    fn node_count_is_6561() {
        let g = build_test_graph();
        assert_eq!(g.node_count(), 6561);
    }
}
