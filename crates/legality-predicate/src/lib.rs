// Legality predicate: the second of three jobs kept separate in DESIGN.md
// §Placement address. Applied after decode; does not affect the rank scheme.
//
// Checks:
//   - King adjacency: two kings must not be on adjacent squares.
//   - Check consistency: side to move MAY be in check; side not to move
//     must NOT be in check (if they were, the previous move was illegal).
//     Do NOT reject all checked positions — that destroys a large legitimate
//     part of the state space.
//   - Castling consistency: asserted castling right requires king and rook
//     on starting squares.
//   - En-passant consistency: ep-available requires an actual ep capture
//     to be available to the side to move (availability, not just that a
//     double-step happened).

use shakmaty::Chess;

/// A specific legality violation found in a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalityViolation {
    /// The two kings are on adjacent squares.
    KingAdjacency,
    /// The side NOT to move is in check (prior move would have been illegal).
    NonMoverInCheck,
    /// A castling right is asserted but the relevant king or rook is not on
    /// its starting square.
    CastlingInconsistency,
    /// En-passant is marked available but no opposing pawn can actually
    /// perform the capture (ep-as-availability semantics).
    EpInconsistency,
}

/// Check a position for all v0 legality conditions.
///
/// Returns `Ok(())` if legal, or the first violation found.
/// Call this after `placement_codec::unrank` to filter the decoded placement.
pub fn check(pos: &Chess) -> Result<(), LegalityViolation> {
    todo!()
}

/// Check only king-adjacency (fast pre-filter before full decode).
pub fn check_king_adjacency(pos: &Chess) -> Result<(), LegalityViolation> {
    todo!()
}
