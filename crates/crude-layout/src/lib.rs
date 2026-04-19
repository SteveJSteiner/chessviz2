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
    /// Strength of local repulsion to de-collide coincident families.
    pub repulsion_strength: f32,
    /// Max world-space distance at which repulsion acts. Families farther apart
    /// than this don't repel each other, keeping the pass O(local) in practice.
    pub repulsion_distance: f32,
    /// 3×8 mode matrix A. Each column is a 3D displacement vector (east, north, radial)
    /// applied per unit of the corresponding φ(F) channel:
    ///   [q_plus, q_minus, r_plus, r_minus, m_plus, m_minus, pawn_density, pawn_imbalance]
    /// The seed x_F = s(F) + A·φ(F) breaks symmetries between families that share
    /// the same coarse seed (same depletion/imbalance/pawn-total) but differ in
    /// piece-type composition profile.
    pub mode_matrix: [[f32; 3]; 8],
    /// Strength of the restoring force pulling each family back toward its
    /// mode-displaced seed, preserving global axis semantics.
    pub anchor_strength: f32,
    /// Per-iteration force clamp in world-space units. Prevents oscillation /
    /// blowup when repulsion is strong near coincident seeds.
    pub max_step: f32,
    /// Number of force-directed iterations (0 = deterministic seed only).
    pub iterations: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            east_scale: 1.0,
            north_scale: 2.0,
            radial_scale: 5.0,
            attraction_strength: 0.005,
            repulsion_strength: 0.4,
            repulsion_distance: 10.0,
            // Columns: q_plus, q_minus, r_plus, r_minus, m_plus, m_minus,
            //          pawn_density, pawn_imbalance
            // Magnitudes chosen to displace ~2–5 world units per unit of φ,
            // enough to break grid symmetry without overwhelming the seed.
            mode_matrix: [
                [0.0,  0.0,  3.0],  // q_plus  → radial (queen-rich = complex positions)
                [0.0,  5.0,  0.0],  // q_minus → north  (queen advantage)
                [3.0,  0.0,  0.0],  // r_plus  → east   (rook endgames = more exchanges)
                [0.0,  3.0,  0.0],  // r_minus → north  (rook advantage)
                [0.0,  0.0, -3.0],  // m_plus  → -radial (minor endgames = simpler)
                [0.0,  2.0,  0.0],  // m_minus → north  (minor advantage)
                [0.0,  0.0,  4.0],  // pawn_density → reinforces radial
                [0.0,  2.0,  0.0],  // pawn_imbalance → north
            ],
            anchor_strength: 0.3,
            max_step: 0.5,
            iterations: 60,
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

    // ── Seed: committed semantic axes + mode displacement A·φ(F) ─────────────
    // s(F) = (depletion·east_scale, material_diff·north_scale, pawn_total·radial_scale)
    // x(F) = s(F) + A·φ(F)
    // A·φ(F) breaks grid symmetry between families sharing the same coarse seed.
    let mut positions: Vec<Vec3> = families
        .iter()
        .map(|rec| {
            let f = &rec.features;
            let seed = Vec3::new(
                f.depletion * config.east_scale,
                f.material_diff * config.north_scale,
                (rec.key.wp as f32 + rec.key.bp as f32) * config.radial_scale,
            );
            let phi = rec.mode.as_array();
            let mode_disp = config.mode_matrix.iter().zip(phi.iter()).fold(
                Vec3::ZERO,
                |acc, (col, &phi_i)| acc + Vec3::new(col[0], col[1], col[2]) * phi_i,
            );
            seed + mode_disp
        })
        .collect();

    // ── Force-directed refinement ─────────────────────────────────────────────
    if config.iterations > 0 {
        // Record seed positions before jitter — used by the anchor force to
        // preserve global axis semantics throughout the FD pass.
        let seeds = positions.clone();

        // Deterministic jitter: break exact seed coincidences so repulsion has
        // something to act on. Multiplicative hash for reproducibility.
        for (i, pos) in positions.iter_mut().enumerate() {
            let h = (i as u32).wrapping_mul(2654435761u32);
            let jx = ((h & 0xFF) as f32 / 255.0 - 0.5) * 0.2;
            let jy = (((h >> 8) & 0xFF) as f32 / 255.0 - 0.5) * 0.2;
            let jz = (((h >> 16) & 0xFF) as f32 / 255.0 - 0.5) * 0.2;
            *pos += Vec3::new(jx, jy, jz);
        }

        let edges: Vec<(usize, usize, f32)> = graph
            .edge_indices()
            .map(|e| {
                let (a, b) = graph.edge_endpoints(e).unwrap();
                (a.index(), b.index(), graph.edge_weight(e).unwrap().layout_weight)
            })
            .collect();

        let rep_dist_sq = config.repulsion_distance * config.repulsion_distance;

        for _ in 0..config.iterations {
            // Local repulsion: de-collides families within repulsion_distance.
            let rep: Vec<Vec3> = if config.repulsion_strength > 0.0 {
                (0..n)
                    .into_par_iter()
                    .map(|i| {
                        let pi = positions[i];
                        let mut f = Vec3::ZERO;
                        for j in 0..n {
                            if i == j { continue; }
                            let delta = pi - positions[j];
                            let d2 = delta.length_squared();
                            if d2 < 1e-8 || d2 > rep_dist_sq { continue; }
                            f += delta * (config.repulsion_strength / d2);
                        }
                        f
                    })
                    .collect()
            } else {
                vec![Vec3::ZERO; n]
            };

            let mut forces = rep;

            // Anchor: restoring force toward seed position.
            // Dominates over repulsion at longer ranges, keeping global
            // axis structure intact (east=irreversibility, etc.).
            if config.anchor_strength > 0.0 {
                for i in 0..n {
                    forces[i] += (seeds[i] - positions[i]) * config.anchor_strength;
                }
            }

            // Attraction along graph edges.
            for &(a, b, w) in &edges {
                let delta = positions[b] - positions[a];
                let attr = delta * (config.attraction_strength * w);
                forces[a] += attr;
                forces[b] -= attr;
            }

            // Apply with per-step clamp to prevent oscillation.
            for i in 0..n {
                let f = forces[i];
                let len = f.length();
                let clamped = if len > config.max_step { f * (config.max_step / len) } else { f };
                positions[i] += clamped;
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

    // Seed-only config: tests that verify the deterministic seed values use
    // iterations=0 so they aren't testing force-directed displacement.
    fn seed_config() -> LayoutConfig {
        LayoutConfig { iterations: 0, ..Default::default() }
    }

    fn setup() -> (Vec<FamilyRecord>, FamilyGraph, LayoutTable) {
        let families = build_table();
        let graph = build_graph(&families);
        let table = compute(&families, &graph, &seed_config());
        (families, graph, table)
    }

    #[test]
    fn layout_count_is_6561() {
        let (_, _, table) = setup();
        assert_eq!(table.layouts.len(), 6561);
    }

    #[test]
    fn seed_east_ordering_matches_depletion() {
        // Mode displacement doesn't change the depletion ordering — more depleted
        // families must still land east of less depleted ones on average.
        let (_, _, table) = setup();
        let start = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index();
        let bare  = FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index();
        assert!(
            table.layouts[bare].center.x > table.layouts[start].center.x,
            "bare family should be east of starting family after mode displacement"
        );
    }

    #[test]
    fn seed_north_sign_matches_material_diff() {
        // q_minus and r_minus channel the queen/rook advantage into north, so
        // white-ahead families must still have positive north.
        let (_, _, table) = setup();
        let white_up = FamilyKey { wnp_band: 7, bnp_band: 3, wp: 4, bp: 4 }.index();
        assert!(table.layouts[white_up].center.y > 0.0, "white-ahead family should have positive north");
        let black_up = FamilyKey { wnp_band: 3, bnp_band: 7, wp: 4, bp: 4 }.index();
        assert!(table.layouts[black_up].center.y < 0.0, "black-ahead family should have negative north");
    }

    #[test]
    fn seed_all_positions_finite() {
        let (_, _, table) = setup();
        for (i, layout) in table.layouts.iter().enumerate() {
            assert!(layout.center.is_finite(), "non-finite position at {i}: {:?}", layout.center);
        }
    }

    #[test]
    fn seed_radial_ordering_matches_pawn_total() {
        // Full-pawn family must have higher radial than same-depletion no-pawn family
        // even after mode displacement (pawn_density term reinforces this).
        let (_, _, table) = setup();
        let full_pawns = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index();
        let no_pawns   = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 0, bp: 0 }.index();
        assert!(
            table.layouts[full_pawns].center.z > table.layouts[no_pawns].center.z,
            "full-pawn family should have higher radial than no-pawn family at same depletion"
        );
    }

    #[test]
    fn mode_displacement_breaks_seed_coincidence() {
        // Two families with the same (depletion, material_diff, pawn_total) but
        // different pawn distributions must have different positions due to
        // the pawn_imbalance channel in A·φ(F).
        let (_, _, table) = setup();
        let a = FamilyKey { wnp_band: 5, bnp_band: 3, wp: 2, bp: 6 }.index();
        let b = FamilyKey { wnp_band: 5, bnp_band: 3, wp: 6, bp: 2 }.index();
        let dist = (table.layouts[a].center - table.layouts[b].center).length();
        assert!(dist > 0.1, "pawn-imbalanced families should differ in position, dist={dist}");
    }

    #[test]
    fn force_directed_preserves_finite_positions() {
        // Small repulsion_distance keeps this fast even at full n=6561.
        let families = build_table();
        let graph = build_graph(&families);
        let config = LayoutConfig {
            iterations: 5,
            repulsion_distance: 3.0,
            ..Default::default()
        };
        let table = compute(&families, &graph, &config);
        for (i, layout) in table.layouts.iter().enumerate() {
            assert!(layout.center.is_finite(), "non-finite position at {i}: {:?}", layout.center);
        }
    }

    #[test]
    fn force_directed_breaks_seed_coincidences() {
        // Two families that share the same seed coordinates must end up distinct
        // after the force-directed pass.
        // (wnp=5, bnp=3, wp=2, bp=6) and (wnp=5, bnp=3, wp=3, bp=5):
        // same depletion, same material_diff, same wp+bp → identical seed.
        let families = build_table();
        let graph = build_graph(&families);
        let config = LayoutConfig {
            iterations: 10,
            repulsion_distance: 5.0,
            ..Default::default()
        };
        let table = compute(&families, &graph, &config);
        let a = FamilyKey { wnp_band: 5, bnp_band: 3, wp: 2, bp: 6 }.index();
        let b = FamilyKey { wnp_band: 5, bnp_band: 3, wp: 3, bp: 5 }.index();
        let dist = (table.layouts[a].center - table.layouts[b].center).length();
        assert!(dist > 0.01, "coincident seed families should be separated by FD pass, got dist={dist}");
    }

    #[test]
    fn extent_budget_scales_with_feature_span() {
        let (families, _, table) = setup();
        let wide = FamilyKey { wnp_band: 7, bnp_band: 3, wp: 0, bp: 0 }.index();
        let narrow = FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index();
        assert!(
            table.layouts[wide].extent_budget.he > table.layouts[narrow].extent_budget.he,
            "wider band should have larger he"
        );
        let expected_he = 0.4 + 0.1 * families[wide].features.feature_span;
        assert!((table.layouts[wide].extent_budget.he - expected_he).abs() < 1e-5);
    }
}
