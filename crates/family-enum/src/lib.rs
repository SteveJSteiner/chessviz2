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
    todo!()
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
    pub fn index(self) -> usize {
        todo!()
    }

    /// Reconstruct a `FamilyKey` from its linear index.
    pub fn from_index(idx: usize) -> Self {
        todo!()
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
/// gives the record for `key`.
pub fn build_table() -> Vec<FamilyRecord> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_count_is_6561() {
        assert_eq!(build_table().len(), 6561);
    }

    #[test]
    fn index_round_trips() {
        for idx in 0..6561_usize {
            let key = FamilyKey::from_index(idx);
            assert_eq!(key.index(), idx, "round-trip failed at {idx}");
        }
    }

    #[test]
    fn band_edges_applied_correctly() {
        assert_eq!(band_of(0), 0);
        assert_eq!(band_of(1), 1);
        assert_eq!(band_of(3), 1);
        assert_eq!(band_of(4), 2);
        assert_eq!(band_of(5), 2);
        assert_eq!(band_of(6), 3);
        assert_eq!(band_of(8), 3);
        assert_eq!(band_of(27), 8);
        assert_eq!(band_of(31), 8);
    }

    #[test]
    fn band_centers_applied_correctly() {
        let table = build_table();
        // Nearest-to-starting-material family: wnp_band=8, bnp_band=8, wp=8, bp=8
        let start_key = FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 };
        let start = table[start_key.index()];
        assert_eq!(start.features.wnp_center, 29.0);
        assert_eq!(start.features.bnp_center, 29.0);

        // Bare-kings family: wnp_band=0, bnp_band=0, wp=0, bp=0
        let bare_key = FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 };
        let bare = table[bare_key.index()];
        assert_eq!(bare.features.wnp_center, 0.0);
        assert_eq!(bare.features.bnp_center, 0.0);
    }

    #[test]
    fn depletion_extremes() {
        let table = build_table();
        // (31−29)+(31−29)+(8−8)+(8−8) = 4
        let start = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index()];
        assert!((start.features.depletion - 4.0).abs() < 1e-6, "{}", start.features.depletion);

        // (31−0)+(31−0)+(8−0)+(8−0) = 78
        let bare = table[FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index()];
        assert!((bare.features.depletion - 78.0).abs() < 1e-6, "{}", bare.features.depletion);
    }

    #[test]
    fn depletion_increases_with_material_loss() {
        let table = build_table();
        let f8 = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 4, bp: 4 }.index()];
        let f7 = table[FamilyKey { wnp_band: 7, bnp_band: 8, wp: 4, bp: 4 }.index()];
        assert!(f7.features.depletion > f8.features.depletion);
    }
}
