// Placement address codec: exact sparse combinadic rank/unrank over non-pawn
// piece placements given a fixed (exact_composition, pawn_basin, stm,
// castling, ep) context. See DESIGN.md §Placement address.
//
// Piece-family order: WK, BK, WQ, BQ, WR, BR, WB, BB, WN, BN.
// Kings first so king-position similarity aligns with rank-neighborhood.
// For families with identical pieces, rank the subset (combination), not
// the tuple — handles indistinguishability without special cases.

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, Square};

/// Opaque combinadic rank identifying a non-pawn piece placement.
pub type PlacementAddress = u64;

/// The context required to interpret a `PlacementAddress`.
///
/// Fixes the free-square set S₀ that the codec operates over.
/// Pawns are pre-placed by the pawn basin; non-pawn piece counts
/// come from the exact composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementContext {
    /// Non-pawn piece counts per side, in piece-family order.
    /// (wk=1, bk=1, wq, bq, wr, br, wb, bb, wn, bn)
    pub piece_counts: [u8; 10],
    /// Bitboard of squares already occupied by pawns.
    pub pawn_squares: u64,
}

/// Encode a chess position as a `PlacementAddress` within the given context.
///
/// The context must match the position (exact composition, pawn basin).
/// Three jobs are kept separate: this function only does exact addressability.
pub fn rank(pos: &Chess, ctx: &PlacementContext) -> PlacementAddress {
    todo!()
}

/// Reconstruct non-pawn piece placement from an address + context.
///
/// Returns the set of (square, color, role) triples for non-pawn pieces.
pub fn unrank(address: PlacementAddress, ctx: &PlacementContext) -> Vec<(Square, Color, shakmaty::Role)> {
    todo!()
}

/// Check that `rank` and `unrank` are inverses for the given position + context.
pub fn roundtrip_check(pos: &Chess, ctx: &PlacementContext) -> bool {
    todo!()
}
