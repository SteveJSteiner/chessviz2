use serde::{Deserialize, Serialize};

/// One entry in the canonical band table.
///
/// All crates that need a band center or span must read from [`BAND_TABLE`];
/// never independently redefine these values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub index: u8,
    /// Inclusive lower bound of the band's piece-value range.
    pub lo: u8,
    /// Inclusive upper bound of the band's piece-value range.
    pub hi: u8,
    /// Representative center value used for layout and derived features.
    pub center: f32,
    /// Semantic width (hi − lo). Drives anisotropic extent in crude-layout.
    pub span: u8,
}

/// Canonical band table, indexed by band index 0..=8.
///
/// Q=9, R=5, B=3, N=3. Max non-pawn material per side = 31.
/// See DESIGN.md §Band table.
pub const BAND_TABLE: [Band; 9] = [
    Band { index: 0, lo: 0,  hi: 0,  center: 0.0,  span: 0 },
    Band { index: 1, lo: 1,  hi: 3,  center: 2.0,  span: 2 },
    Band { index: 2, lo: 4,  hi: 5,  center: 4.5,  span: 1 },
    Band { index: 3, lo: 6,  hi: 8,  center: 7.0,  span: 2 },
    Band { index: 4, lo: 9,  hi: 11, center: 10.0, span: 2 },
    Band { index: 5, lo: 12, hi: 15, center: 13.5, span: 3 },
    Band { index: 6, lo: 16, hi: 20, center: 18.0, span: 4 },
    Band { index: 7, lo: 21, hi: 26, center: 23.5, span: 5 },
    Band { index: 8, lo: 27, hi: 31, center: 29.0, span: 4 },
];

/// Map a non-pawn piece-value sum (0..=31) to its band index.
pub fn band_of(wnp_value: u8) -> u8 {
    match wnp_value {
        0       => 0,
        1..=3   => 1,
        4..=5   => 2,
        6..=8   => 3,
        9..=11  => 4,
        12..=15 => 5,
        16..=20 => 6,
        21..=26 => 7,
        27..=31 => 8,
        v => panic!("non-pawn material value {v} exceeds theoretical maximum of 31"),
    }
}

/// Family key: material composition binned by the band table.
///
/// `wnp_band`, `bnp_band` ∈ 0..=8; `wp`, `bp` ∈ 0..=8.
/// Family count: 9 × 9 × 9 × 9 = 6561.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FamilyKey {
    pub wnp_band: u8,
    pub bnp_band: u8,
    pub wp: u8,
    pub bp: u8,
}

impl FamilyKey {
    /// Linear index into the 6561-entry family table (wnp_band major, bp minor).
    #[inline]
    pub fn index(self) -> usize {
        self.wnp_band as usize * 729
            + self.bnp_band as usize * 81
            + self.wp as usize * 9
            + self.bp as usize
    }

    /// Reconstruct a `FamilyKey` from its linear index.
    #[inline]
    pub fn from_index(idx: usize) -> Self {
        debug_assert!(idx < 6561, "family index {idx} out of range");
        FamilyKey {
            wnp_band: (idx / 729) as u8,
            bnp_band: ((idx % 729) / 81) as u8,
            wp:       ((idx % 81) / 9) as u8,
            bp:       (idx % 9) as u8,
        }
    }
}

/// Derived scalar features for a family, computed from the key + [`BAND_TABLE`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FamilyFeatures {
    /// Band-center piece-value sum for white non-pawns.
    pub wnp_center: f32,
    /// Band-center piece-value sum for black non-pawns.
    pub bnp_center: f32,
    /// Total material: wnp_center + bnp_center + wp + bp (pawn = 1 point).
    pub total_material: f32,
    /// Material imbalance: wnp_center − bnp_center. Positive = white ahead.
    pub material_diff: f32,
    /// Phase estimate ∈ [0, 1]: 0 = full material (opening), 1 = fully depleted.
    pub phase_estimate: f32,
    /// Compositional depletion from starting material; used as the east coordinate.
    /// depletion = (31−wnp_center) + (31−bnp_center) + (8−wp) + (8−bp). Max = 78.
    pub depletion: f32,
    /// Semantic width of the wider NP band (max of wnp span, bnp span).
    /// Drives anisotropic extent budget in crude-layout.
    pub feature_span: f32,
}

/// One row in the family table: key + derived features.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FamilyRecord {
    pub key: FamilyKey,
    pub features: FamilyFeatures,
}

/// Build the complete 6561-entry family table.
///
/// Records are stored in `key.index()` order so that `table[key.index()]`
/// gives the record for `key`. All feature computation is pure arithmetic
/// over `BAND_TABLE`; no IO, no randomness.
pub fn build_table() -> Vec<FamilyRecord> {
    let mut records = Vec::with_capacity(6561);
    for wnp_band in 0u8..9 {
        for bnp_band in 0u8..9 {
            for wp in 0u8..9 {
                for bp in 0u8..9 {
                    let key = FamilyKey { wnp_band, bnp_band, wp, bp };
                    let wb = &BAND_TABLE[wnp_band as usize];
                    let bb = &BAND_TABLE[bnp_band as usize];

                    let wnp_center    = wb.center;
                    let bnp_center    = bb.center;
                    let total_material = wnp_center + bnp_center + wp as f32 + bp as f32;
                    let material_diff  = wnp_center - bnp_center;
                    let depletion      = (31.0 - wnp_center)
                                       + (31.0 - bnp_center)
                                       + (8.0  - wp as f32)
                                       + (8.0  - bp as f32);
                    let phase_estimate = depletion / 78.0;
                    let feature_span   = wb.span.max(bb.span) as f32;

                    records.push(FamilyRecord {
                        key,
                        features: FamilyFeatures {
                            wnp_center,
                            bnp_center,
                            total_material,
                            material_diff,
                            phase_estimate,
                            depletion,
                            feature_span,
                        },
                    });
                }
            }
        }
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Basic count and structure ─────────────────────────────────────────────

    #[test]
    fn table_count_is_6561() {
        assert_eq!(build_table().len(), 6561);
    }

    #[test]
    fn table_index_order_matches_key_index() {
        let table = build_table();
        for (i, rec) in table.iter().enumerate() {
            assert_eq!(rec.key.index(), i, "record at slot {i} has key.index()={}", rec.key.index());
        }
    }

    #[test]
    fn index_round_trips() {
        for idx in 0..6561_usize {
            let key = FamilyKey::from_index(idx);
            assert_eq!(key.index(), idx, "round-trip failed at {idx}");
        }
    }

    // ── Band table coverage ───────────────────────────────────────────────────

    #[test]
    fn band_table_entries_are_consistent() {
        for b in &BAND_TABLE {
            assert_eq!(b.span, b.hi - b.lo, "span mismatch for band {}", b.index);
        }
    }

    // ── band_of edge values ───────────────────────────────────────────────────

    #[test]
    fn band_edges_applied_correctly() {
        // Each band's lo and hi must map to that band.
        for b in &BAND_TABLE {
            assert_eq!(band_of(b.lo), b.index, "lo edge failed for band {}", b.index);
            assert_eq!(band_of(b.hi), b.index, "hi edge failed for band {}", b.index);
        }
        // A selection of interior and boundary values.
        assert_eq!(band_of(0),  0);
        assert_eq!(band_of(1),  1);
        assert_eq!(band_of(3),  1);
        assert_eq!(band_of(4),  2);
        assert_eq!(band_of(5),  2);
        assert_eq!(band_of(6),  3);
        assert_eq!(band_of(8),  3);
        assert_eq!(band_of(9),  4);
        assert_eq!(band_of(11), 4);
        assert_eq!(band_of(12), 5);
        assert_eq!(band_of(15), 5);
        assert_eq!(band_of(16), 6);
        assert_eq!(band_of(20), 6);
        assert_eq!(band_of(21), 7);
        assert_eq!(band_of(26), 7);
        assert_eq!(band_of(27), 8);
        assert_eq!(band_of(31), 8);
    }

    #[test]
    fn band_of_is_exhaustive_and_monotone() {
        let mut last_band = 0u8;
        for v in 0u8..=31 {
            let b = band_of(v);
            assert!(b >= last_band, "band_of not monotone at v={v}");
            last_band = b;
        }
    }

    // ── Derived feature correctness ───────────────────────────────────────────

    #[test]
    fn band_centers_applied_correctly() {
        let table = build_table();

        // Band 8 center = 29.0; band 0 center = 0.0
        let start = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index()];
        assert_eq!(start.features.wnp_center, 29.0);
        assert_eq!(start.features.bnp_center, 29.0);

        let bare = table[FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index()];
        assert_eq!(bare.features.wnp_center, 0.0);
        assert_eq!(bare.features.bnp_center, 0.0);

        // Band 5 center = 13.5
        let mid = table[FamilyKey { wnp_band: 5, bnp_band: 5, wp: 4, bp: 4 }.index()];
        assert!((mid.features.wnp_center - 13.5).abs() < 1e-6);
    }

    #[test]
    fn depletion_extremes() {
        let table = build_table();

        // (31−29)+(31−29)+(8−8)+(8−8) = 4
        let start = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index()];
        assert!(
            (start.features.depletion - 4.0).abs() < 1e-5,
            "expected 4.0 got {}",
            start.features.depletion
        );

        // (31−0)+(31−0)+(8−0)+(8−0) = 78
        let bare = table[FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index()];
        assert!(
            (bare.features.depletion - 78.0).abs() < 1e-5,
            "expected 78.0 got {}",
            bare.features.depletion
        );
    }

    #[test]
    fn depletion_increases_with_material_loss() {
        let table = build_table();
        // Dropping white's NP band from 8 to 7 must increase depletion.
        let f8 = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 4, bp: 4 }.index()];
        let f7 = table[FamilyKey { wnp_band: 7, bnp_band: 8, wp: 4, bp: 4 }.index()];
        assert!(f7.features.depletion > f8.features.depletion);

        // Losing a pawn also increases depletion.
        let p4 = table[FamilyKey { wnp_band: 5, bnp_band: 5, wp: 4, bp: 4 }.index()];
        let p3 = table[FamilyKey { wnp_band: 5, bnp_band: 5, wp: 3, bp: 4 }.index()];
        assert!(p3.features.depletion > p4.features.depletion);
        assert!((p3.features.depletion - p4.features.depletion - 1.0).abs() < 1e-5);
    }

    #[test]
    fn phase_estimate_is_normalized() {
        let table = build_table();
        for rec in &table {
            let ph = rec.features.phase_estimate;
            assert!(
                ph >= 0.0 && ph <= 1.0,
                "phase_estimate {ph} out of [0,1] for key {:?}",
                rec.key
            );
        }
    }

    #[test]
    fn material_diff_sign_convention() {
        let table = build_table();
        // White ahead: wnp_band > bnp_band → material_diff > 0
        let white_up = table[FamilyKey { wnp_band: 7, bnp_band: 3, wp: 4, bp: 4 }.index()];
        assert!(white_up.features.material_diff > 0.0);

        // Black ahead: wnp_band < bnp_band → material_diff < 0
        let black_up = table[FamilyKey { wnp_band: 3, bnp_band: 7, wp: 4, bp: 4 }.index()];
        assert!(black_up.features.material_diff < 0.0);

        // Equal: material_diff = 0
        let equal = table[FamilyKey { wnp_band: 5, bnp_band: 5, wp: 4, bp: 4 }.index()];
        assert!((equal.features.material_diff).abs() < 1e-6);
    }

    #[test]
    fn feature_span_is_max_of_band_spans() {
        let table = build_table();
        // Band 7 span=5, band 3 span=2 → feature_span should be 5
        let rec = table[FamilyKey { wnp_band: 7, bnp_band: 3, wp: 0, bp: 0 }.index()];
        assert!((rec.features.feature_span - 5.0).abs() < 1e-6);

        // Band 0 span=0 on both sides → feature_span=0
        let bare = table[FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index()];
        assert!((bare.features.feature_span - 0.0).abs() < 1e-6);
    }
}
