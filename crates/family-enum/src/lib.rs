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

/// Per-band statistics over the starting-limit NP composition enumeration.
/// Family-layer composition priors — not exact-composition truth, which is
/// promotion-aware and finer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BandCompositionStats {
    /// P(queen present) over starting-limit compositions in this band.
    pub q: f32,
    /// E[rook count] over starting-limit compositions in this band.
    pub r: f32,
    /// P(rookless and ≥2 minors) over starting-limit compositions.
    pub m: f32,
}

/// Precompute per-band composition stats for all 9 bands.
pub fn band_composition_stats() -> [BandCompositionStats; 9] {
    std::array::from_fn(|band_idx| {
        let b = &BAND_TABLE[band_idx];
        let mut count = 0u32;
        let mut q_count = 0u32;
        let mut r_sum = 0u32;
        let mut m_count = 0u32;
        for wq in 0u8..=1 {
            for wr in 0u8..=2 {
                for wb in 0u8..=2 {
                    for wn in 0u8..=2 {
                        let v = 9 * wq + 5 * wr + 3 * wb + 3 * wn;
                        if v < b.lo || v > b.hi { continue; }
                        count += 1;
                        if wq >= 1 { q_count += 1; }
                        r_sum += wr as u32;
                        if wr == 0 && (wn + wb) >= 2 { m_count += 1; }
                    }
                }
            }
        }
        let n = count as f32;
        BandCompositionStats {
            q: if n > 0.0 { q_count as f32 / n } else { 0.0 },
            r: if n > 0.0 { r_sum as f32 / n } else { 0.0 },
            m: if n > 0.0 { m_count as f32 / n } else { 0.0 },
        }
    })
}

/// Family-layer composition prior feature vector φ(F).
/// Symmetric (+) and antisymmetric (−) channels over per-band stats.
/// "Prior" because it uses band-level approximations, not the promotion-aware
/// exact-composition layer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FamilyModeFeatures {
    pub q_plus: f32,         // q(wb) + q(bb)
    pub q_minus: f32,        // q(wb) - q(bb)
    pub r_plus: f32,         // r(wb) + r(bb)
    pub r_minus: f32,        // r(wb) - r(bb)
    pub m_plus: f32,         // m(wb) + m(bb)
    pub m_minus: f32,        // m(wb) - m(bb)
    pub pawn_density: f32,   // (wp + bp) / 16
    pub pawn_imbalance: f32, // (wp - bp) / 8
}

impl FamilyModeFeatures {
    pub fn as_array(&self) -> [f32; 8] {
        [self.q_plus, self.q_minus, self.r_plus, self.r_minus,
         self.m_plus, self.m_minus, self.pawn_density, self.pawn_imbalance]
    }
}

/// Minimum non-pawn pieces that must have been removed from the starting 7
/// (Q, R, R, B, B, N, N) to land in each NP band, computed greedily by
/// removing the highest-value piece first.
///
/// Indexed by band index 0..=8. Both sides share the same table (symmetric).
pub const WNP_MIN_PIECES_REMOVED: [u32; 9] = [
    7, // band 0 [0]:     all 7 removed
    6, // band 1 [1–3]:   leave 1 N or B
    6, // band 2 [4–5]:   leave 1 R (no 2-piece combo ≤ 5)
    5, // band 3 [6–8]:   leave N+N / N+B / B+B
    4, // band 4 [9–11]:  leave B+N+N / etc.
    3, // band 5 [12–15]: leave R+B+B / etc.
    2, // band 6 [16–20]: leave R+R+B+B / etc.
    1, // band 7 [21–26]: leave all but Q (→ 22)
    0, // band 8 [27–31]: full starting set
];

fn binary_entropy(p: f32) -> f32 {
    if p <= 0.0 || p >= 1.0 { return 0.0; }
    -p * p.log2() - (1.0 - p) * (1.0 - p).log2()
}

/// Weighted strategic entropy S(F) over the exact admissible piece-count
/// realizations inside a family identified by (wnp_band, bnp_band).
///
/// Enumerates all (wQ,wR,wB,wN) × (bQ,bR,bB,bN) pairs (Q∈{0,1}, R/B/N∈{0,1,2})
/// whose NP point totals land in the respective bands, then computes five binary
/// strategic indicators and returns their weighted entropy sum.
/// Result depends only on (wnp_band, bnp_band); pawn counts do not affect it.
pub fn strategic_entropy_score(wnp_band: u8, bnp_band: u8) -> f32 {
    let wband = &BAND_TABLE[wnp_band as usize];
    let bband = &BAND_TABLE[bnp_band as usize];

    let mut white: Vec<(u8, u8, u8, u8)> = Vec::new(); // (wQ, wR, wB, wN)
    for wq in 0u8..=1 {
        for wr in 0u8..=2 {
            for wb in 0u8..=2 {
                for wn in 0u8..=2 {
                    let v = 9 * wq + 5 * wr + 3 * wb + 3 * wn;
                    if v >= wband.lo && v <= wband.hi {
                        white.push((wq, wr, wb, wn));
                    }
                }
            }
        }
    }

    let mut black: Vec<(u8, u8, u8, u8)> = Vec::new(); // (bQ, bR, bB, bN)
    for bq in 0u8..=1 {
        for br in 0u8..=2 {
            for bb in 0u8..=2 {
                for bn in 0u8..=2 {
                    let v = 9 * bq + 5 * br + 3 * bb + 3 * bn;
                    if v >= bband.lo && v <= bband.hi {
                        black.push((bq, br, bb, bn));
                    }
                }
            }
        }
    }

    let n = (white.len() * black.len()) as f32;
    if n == 0.0 { return 0.0; }

    let mut cnt_q_any = 0u32;
    let mut cnt_r_any = 0u32;
    let mut cnt_m_rich = 0u32;
    let mut cnt_bp_any = 0u32;
    let mut cnt_h_dom = 0u32;

    for &(wq, wr, wb, wn) in &white {
        for &(bq, br, bb, bn) in &black {
            if wq + bq > 0 { cnt_q_any += 1; }
            if wr + br > 0 { cnt_r_any += 1; }
            if (wb + wn + bb + bn) >= 3 { cnt_m_rich += 1; }
            if wb == 2 || bb == 2 { cnt_bp_any += 1; }
            let heavy = 5 * (wr + br) as u32 + 9 * (wq + bq) as u32;
            let minor = 3 * (wb + wn + bb + bn) as u32;
            if heavy > minor { cnt_h_dom += 1; }
        }
    }

    let p_q  = cnt_q_any  as f32 / n;
    let p_r  = cnt_r_any  as f32 / n;
    let p_m  = cnt_m_rich as f32 / n;
    let p_bp = cnt_bp_any as f32 / n;
    let p_h  = cnt_h_dom  as f32 / n;

    1.50 * binary_entropy(p_q)
        + 1.50 * binary_entropy(p_r)
        + 1.00 * binary_entropy(p_m)
        + 1.00 * binary_entropy(p_bp)
        + 1.25 * binary_entropy(p_h)
}

/// Count of distinct starting-limit NP piece-count tuples (wn,wb,wr,wq)
/// with wn,wb,wr ∈ 0..=2, wq ∈ 0..=1 whose value (3wn+3wb+5wr+9wq)
/// falls in the band's [lo, hi] range.
fn wnp_band_composition_count(band_idx: u8) -> u32 {
    let b = &BAND_TABLE[band_idx as usize];
    let mut n = 0u32;
    for wq in 0u8..=1 {
        for wr in 0u8..=2 {
            for wb in 0u8..=2 {
                for wn in 0u8..=2 {
                    let v = 9 * wq + 5 * wr + 3 * wb + 3 * wn;
                    if v >= b.lo && v <= b.hi {
                        n += 1;
                    }
                }
            }
        }
    }
    n
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
    /// Coarse prior on family occupancy: product of the counts of starting-limit
    /// NP piece-count tuples that fall in each side's band. Excludes promotions.
    /// Named "prior" because it undercounts promoted-piece compositions.
    pub family_mass_prior: u32,
    /// Lower bound on the minimum number of capture plies needed to reach this
    /// family from the starting position. Based on piece counts lost, not point
    /// values — one capture ply removes exactly one piece regardless of its value.
    /// = WNP_MIN_PIECES_REMOVED[wnp_band] + WNP_MIN_PIECES_REMOVED[bnp_band]
    ///   + (8 − wp) + (8 − bp). Returns 0 for the starting family.
    pub min_capture_bound: u32,
    /// Weighted strategic entropy over the exact admissible piece-count
    /// realizations inside this family. Measures how many distinct strategic
    /// regimes (queen game, rook game, minor-piece game, bishop-pair, heavy
    /// dominance) coexist within the family's composition space.
    /// High = strategically mixed (many regimes present); low = type-pure.
    /// Depends only on (wnp_band, bnp_band); pawn counts do not affect it.
    pub strategic_entropy: f32,
}

/// One row in the family table: key + derived features + composition prior.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FamilyRecord {
    pub key: FamilyKey,
    pub features: FamilyFeatures,
    pub mode: FamilyModeFeatures,
}

/// Build the complete 6561-entry family table.
///
/// Records are stored in `key.index()` order so that `table[key.index()]`
/// gives the record for `key`. All feature computation is pure arithmetic
/// over `BAND_TABLE`; no IO, no randomness.
pub fn build_table() -> Vec<FamilyRecord> {
    let comp_counts: [u32; 9] =
        std::array::from_fn(|i| wnp_band_composition_count(i as u8));
    let band_stats = band_composition_stats();

    let mut records = Vec::with_capacity(6561);
    for wnp_band in 0u8..9 {
        for bnp_band in 0u8..9 {
            for wp in 0u8..9 {
                for bp in 0u8..9 {
                    let key = FamilyKey { wnp_band, bnp_band, wp, bp };
                    let wb = &BAND_TABLE[wnp_band as usize];
                    let bb = &BAND_TABLE[bnp_band as usize];
                    let ws = &band_stats[wnp_band as usize];
                    let bs = &band_stats[bnp_band as usize];

                    let wnp_center     = wb.center;
                    let bnp_center     = bb.center;
                    let total_material = wnp_center + bnp_center + wp as f32 + bp as f32;
                    let material_diff  = wnp_center - bnp_center;
                    let depletion      = (31.0 - wnp_center)
                                       + (31.0 - bnp_center)
                                       + (8.0  - wp as f32)
                                       + (8.0  - bp as f32);
                    let phase_estimate = depletion / 78.0;
                    let feature_span   = wb.span.max(bb.span) as f32;
                    let family_mass_prior =
                        comp_counts[wnp_band as usize] * comp_counts[bnp_band as usize];
                    let min_capture_bound =
                        WNP_MIN_PIECES_REMOVED[wnp_band as usize]
                        + WNP_MIN_PIECES_REMOVED[bnp_band as usize]
                        + (8 - wp) as u32
                        + (8 - bp) as u32;

                    let mode = FamilyModeFeatures {
                        q_plus:         ws.q + bs.q,
                        q_minus:        ws.q - bs.q,
                        r_plus:         ws.r + bs.r,
                        r_minus:        ws.r - bs.r,
                        m_plus:         ws.m + bs.m,
                        m_minus:        ws.m - bs.m,
                        pawn_density:   (wp as f32 + bp as f32) / 16.0,
                        pawn_imbalance: (wp as f32 - bp as f32) / 8.0,
                    };

                    let strategic_entropy =
                        strategic_entropy_score(wnp_band, bnp_band);

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
                            family_mass_prior,
                            min_capture_bound,
                            strategic_entropy,
                        },
                        mode,
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
    fn family_mass_prior_starting_family_is_nine() {
        let table = build_table();
        // Band 8 [27,31] has 3 NP compositions from starting limits:
        //   (wn=2,wb=2,wr=2,wq=1)=31, (wn=1,wb=2,wr=2,wq=1)=28, (wn=2,wb=1,wr=2,wq=1)=28
        // mass_prior = 3 * 3 = 9 (both sides symmetric).
        let rec = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index()];
        assert_eq!(rec.features.family_mass_prior, 9,
            "starting family should have mass_prior=9, got {}", rec.features.family_mass_prior);
    }

    #[test]
    fn family_mass_prior_bare_family_is_one() {
        let table = build_table();
        // Bare family (wnp_band=0, bnp_band=0, wp=0, bp=0):
        // Only one NP composition in band 0: the empty set (0,0,0,0).
        let rec = table[FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index()];
        assert_eq!(rec.features.family_mass_prior, 1);
    }

    #[test]
    fn family_mass_prior_is_nonzero_for_all_families() {
        let table = build_table();
        for rec in &table {
            assert!(rec.features.family_mass_prior > 0,
                "family_mass_prior is 0 for {:?}", rec.key);
        }
    }

    #[test]
    fn family_mass_prior_peaks_at_middle_bands() {
        let table = build_table();
        // Middle bands (4-6) have more compositions than extremes (0,8).
        let mid = table[FamilyKey { wnp_band: 5, bnp_band: 5, wp: 4, bp: 4 }.index()];
        let start = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index()];
        let bare = table[FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index()];
        assert!(mid.features.family_mass_prior > start.features.family_mass_prior);
        assert!(mid.features.family_mass_prior > bare.features.family_mass_prior);
    }

    #[test]
    fn min_capture_bound_starting_family_is_zero() {
        let table = build_table();
        let rec = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index()];
        assert_eq!(rec.features.min_capture_bound, 0,
            "starting family reachable at ply 0, got {}", rec.features.min_capture_bound);
    }

    #[test]
    fn min_capture_bound_bare_family_is_30() {
        let table = build_table();
        // All 7+7 NP pieces plus 8+8 pawns must be captured: 30 total.
        let rec = table[FamilyKey { wnp_band: 0, bnp_band: 0, wp: 0, bp: 0 }.index()];
        assert_eq!(rec.features.min_capture_bound, 30,
            "bare family needs 30 captures, got {}", rec.features.min_capture_bound);
    }

    #[test]
    fn min_capture_bound_increases_with_depletion() {
        let table = build_table();
        // Losing white's queen (band 8→7) must increase min_capture_bound by 1.
        let f8 = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 8, bp: 8 }.index()];
        let f7 = table[FamilyKey { wnp_band: 7, bnp_band: 8, wp: 8, bp: 8 }.index()];
        assert_eq!(f7.features.min_capture_bound, f8.features.min_capture_bound + 1);
        // Losing a pawn also costs exactly 1 capture ply.
        let fp = table[FamilyKey { wnp_band: 8, bnp_band: 8, wp: 7, bp: 8 }.index()];
        assert_eq!(fp.features.min_capture_bound, f8.features.min_capture_bound + 1);
    }

    #[test]
    fn wnp_composition_counts_match_expected() {
        // Verify the per-band counts against the manually-derived table.
        let expected: [u32; 9] = {
            let mut e = [0u32; 9];
            for i in 0..9 { e[i] = wnp_band_composition_count(i as u8); }
            e
        };
        // Band 0: only (0,0,0,0) → 1
        assert_eq!(expected[0], 1);
        // Band 1: 1N, 1B → 2
        assert_eq!(expected[1], 2);
        // Band 2: 1R → 1
        assert_eq!(expected[2], 1);
        // Band 8 [27,31]: (2,2,2,1)=31, (1,2,2,1)=28, (2,1,2,1)=28 → 3
        assert_eq!(expected[8], 3);
        // All must be nonzero
        for (i, &c) in expected.iter().enumerate() {
            assert!(c > 0, "band {i} has 0 compositions");
        }
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
