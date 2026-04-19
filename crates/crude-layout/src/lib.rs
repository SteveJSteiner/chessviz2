use family_enum::FamilyRecord;
use family_graph::FamilyGraph;
use glam::{Mat3, Vec3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Half-extents per axis for a family cell.
/// East (hE), north/south (hN), radial (hR). See DESIGN.md §Split rule principles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExtentBudget {
    pub he: f32,
    pub hn: f32,
    pub hr: f32,
}

/// R³ layout record for one family.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FamilyLayout {
    /// Centroid in R³.
    pub center: Vec3,
    /// Local orientation frame (column vectors = east, north, radial axes).
    pub orientation: Mat3,
    /// Half-extents in the local frame.
    pub extent_budget: ExtentBudget,
}

/// Complete layout table, parallel to the family table (indexed by `key.index()`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutTable {
    pub layouts: Vec<FamilyLayout>,
}

/// Tunable parameters for the crude layout pass.
/// V0 uses simple deterministic rules; these constants drive the formulas.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Scale factor mapping depletion [0, 78] onto the east axis.
    pub east_scale: f32,
    /// Scale factor mapping material_diff onto the north/south axis.
    pub north_scale: f32,
    /// Scale factor mapping total pawn count [0, 16] onto the radial axis.
    /// Default 5.0 gives radial ∈ [0, 80], comparable to east ∈ [4, 78].
    pub radial_scale: f32,
    /// Strength of graph-edge attraction in the force-directed pass.
    pub attraction_strength: f32,
    /// Strength of local repulsion to prevent cell overlap (0 = disabled).
    pub repulsion_strength: f32,
    /// Number of force-directed iterations (0 = deterministic placement only).
    pub iterations: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            east_scale: 1.0,
            north_scale: 2.0,
            radial_scale: 5.0,
            attraction_strength: 0.05,
            repulsion_strength: 0.0,
            iterations: 0,
        }
    }
}

/// Compute the crude layout from family records and the transition graph.
///
/// Axis assignment (DESIGN.md §Split rule principles):
/// - East   = depletion × east_scale   (compositional irreversibility)
/// - North  = material_diff × north_scale  (outcome pull)
/// - Radial = (WP + BP) × radial_scale  (total pawn count — genuinely
///            independent of depletion, unlike phase_estimate which is
///            depletion/78 and collapses to the east axis)
///
/// An optional force-directed pass applies edge attraction and (if
/// repulsion_strength > 0) pairwise repulsion.
pub fn compute(
    families: &[FamilyRecord],
    graph: &FamilyGraph,
    config: &LayoutConfig,
) -> LayoutTable {
    let n = families.len();

    // ── Deterministic initial positions ──────────────────────────────────────
    let mut positions: Vec<Vec3> = families
        .iter()
        .map(|rec| {
            let f = &rec.features;
            Vec3::new(
                f.depletion * config.east_scale,
                f.material_diff * config.north_scale,
                (rec.key.wp as f32 + rec.key.bp as f32) * config.radial_scale,
            )
        })
        .collect();

    // ── Force-directed refinement ─────────────────────────────────────────────
    if config.iterations > 0 {
        let edges: Vec<(usize, usize, f32)> = graph
            .edge_indices()
            .map(|e| {
                let (a, b) = graph.edge_endpoints(e).unwrap();
                (a.index(), b.index(), graph.edge_weight(e).unwrap().layout_weight)
            })
            .collect();

        for _ in 0..config.iterations {
            let mut forces = vec![Vec3::ZERO; n];

            // Repulsion (O(n²), gated by repulsion_strength > 0)
            if config.repulsion_strength > 0.0 {
                let rep: Vec<Vec3> = (0..n)
                    .into_par_iter()
                    .map(|i| {
                        let pi = positions[i];
                        let mut f = Vec3::ZERO;
                        for j in 0..n {
                            if i == j {
                                continue;
                            }
                            let delta = pi - positions[j];
                            let d2 = delta.length_squared();
                            if d2 > 1e-6 {
                                f += delta * (config.repulsion_strength / d2);
                            }
                        }
                        f
                    })
                    .collect();
                for i in 0..n {
                    forces[i] += rep[i];
                }
            }

            // Attraction along graph edges
            for &(a, b, w) in &edges {
                let delta = positions[b] - positions[a];
                let attr = delta * (config.attraction_strength * w);
                forces[a] += attr;
                forces[b] -= attr;
            }

            for i in 0..n {
                positions[i] += forces[i];
            }
        }
    }

    // ── Assemble layout records ───────────────────────────────────────────────
    let layouts = families
        .iter()
        .enumerate()
        .map(|(i, rec)| {
            let span = rec.features.feature_span;
            FamilyLayout {
                center: positions[i],
                orientation: Mat3::IDENTITY,
                extent_budget: ExtentBudget {
                    // east half-extent grows with the band's semantic width
                    he: 0.4 + 0.1 * span,
                    hn: 0.5,
                    hr: 0.5,
                },
            }
        })
        .collect();

    LayoutTable { layouts }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use family_enum::{build_table, FamilyKey};
    use family_graph::build_graph;

    fn setup() -> (Vec<FamilyRecord>, FamilyGraph, LayoutTable) {
        let families = build_table();
        let graph = build_graph(&families);
        let table = compute(&families, &graph, &LayoutConfig::default());
        (families, graph, table)
    }

    #[test]
    fn layout_count_is_6561() {
        let (_, _, table) = setup();
        assert_eq!(table.layouts.len(), 6561);
    }

    #[test]
    fn east_coordinate_increases_with_depletion() {
        let (families, _, table) = setup();
        // Minimum depletion: starting family (8,8,8,8) — depletion ≈ 4.0
        // Maximum depletion: bare family (0,0,0,0) — depletion = 78.0
        let start = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index();
        let bare = FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index();
        let east_start = table.layouts[start].center.x;
        let east_bare = table.layouts[bare].center.x;
        assert!(
            east_bare > east_start,
            "bare family east {east_bare} should exceed starting-family east {east_start}"
        );
        // Verify east = depletion * east_scale for default config
        let cfg = LayoutConfig::default();
        let expected_start = families[start].features.depletion * cfg.east_scale;
        let expected_bare = families[bare].features.depletion * cfg.east_scale;
        assert!((east_start - expected_start).abs() < 1e-4);
        assert!((east_bare - expected_bare).abs() < 1e-4);
    }

    #[test]
    fn north_sign_matches_material_diff() {
        let (_, _, table) = setup();
        // White-ahead family: wnp_band=7, bnp_band=3 → material_diff > 0 → north > 0
        let white_up = FamilyKey { wnp_band: 7, bnp_band: 3, wp: 4, bp: 4 }.index();
        assert!(
            table.layouts[white_up].center.y > 0.0,
            "white-ahead family should have positive north"
        );
        // Black-ahead: north < 0
        let black_up = FamilyKey { wnp_band: 3, bnp_band: 7, wp: 4, bp: 4 }.index();
        assert!(
            table.layouts[black_up].center.y < 0.0,
            "black-ahead family should have negative north"
        );
        // Equal material: north = 0
        let equal = FamilyKey { wnp_band: 5, bnp_band: 5, wp: 4, bp: 4 }.index();
        assert!(
            table.layouts[equal].center.y.abs() < 1e-5,
            "equal-material family should have north ≈ 0"
        );
    }

    #[test]
    fn radial_coordinate_is_finite_and_non_negative() {
        let (_, _, table) = setup();
        for (i, layout) in table.layouts.iter().enumerate() {
            let r = layout.center.z;
            assert!(r.is_finite(), "non-finite radial at index {i}: {r}");
            assert!(r >= 0.0, "negative radial at index {i}: {r}");
        }
    }

    #[test]
    fn radial_is_independent_of_east() {
        // Families with the same depletion but different pawn totals must have
        // different radial coordinates, proving radial is not just a scaled east.
        // (8,8,8,8): depletion=4, WP+BP=16 → radial = 16 * 5 = 80
        // (0,0,0,0): depletion=78, WP+BP=0  → radial = 0
        // (8,8,0,0): depletion=4,  WP+BP=0  → radial = 0  ← same east, different radial
        let (_, _, table) = setup();
        let cfg = LayoutConfig::default();
        let full_pawns = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index();
        let no_pawns   = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 0, bp: 0 }.index();
        // Same depletion (same east), but full_pawns has radial=80, no_pawns has radial=0
        let r_full = table.layouts[full_pawns].center.z;
        let r_none = table.layouts[no_pawns].center.z;
        assert!(
            (r_full - 16.0 * cfg.radial_scale).abs() < 1e-4,
            "full-pawn radial should be 16*radial_scale, got {r_full}"
        );
        assert!(
            r_none.abs() < 1e-4,
            "no-pawn radial should be 0, got {r_none}"
        );
        assert!(
            r_full > r_none,
            "same-depletion families with different pawn counts must differ in radial"
        );
    }

    #[test]
    fn force_directed_preserves_finite_positions() {
        let families = build_table();
        let graph = build_graph(&families);
        let config = LayoutConfig { iterations: 10, attraction_strength: 0.01, ..Default::default() };
        let table = compute(&families, &graph, &config);
        for (i, layout) in table.layouts.iter().enumerate() {
            let c = layout.center;
            assert!(
                c.is_finite(),
                "non-finite position at index {i}: {c:?}"
            );
        }
    }

    #[test]
    fn extent_budget_scales_with_feature_span() {
        let (families, _, table) = setup();
        // Band 7 has span 5, band 0 has span 0 → he should differ
        let wide = FamilyKey { wnp_band: 7, bnp_band: 3, wp: 0, bp: 0 }.index();
        let narrow = FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index();
        assert!(
            table.layouts[wide].extent_budget.he > table.layouts[narrow].extent_budget.he,
            "wider band should have larger he"
        );
        // feature_span for (7,3): max(span_7, span_3) = max(5, 2) = 5
        // he = 0.4 + 0.1 * 5 = 0.9
        let expected_he = 0.4 + 0.1 * families[wide].features.feature_span;
        assert!((table.layouts[wide].extent_budget.he - expected_he).abs() < 1e-5);
    }
}
