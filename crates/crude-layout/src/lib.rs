use family_enum::FamilyRecord;
use family_graph::FamilyGraph;
use glam::{Mat3, Vec3};
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
    /// Scale factor mapping phase_estimate onto the radial axis.
    pub radial_scale: f32,
    /// Strength of graph-edge attraction in the force-directed pass.
    pub attraction_strength: f32,
    /// Strength of local repulsion to prevent cell overlap.
    pub repulsion_strength: f32,
    /// Number of force-directed iterations (0 = deterministic only).
    pub iterations: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        todo!()
    }
}

/// Compute the crude layout from family records and the transition graph.
/// See DESIGN.md §Crude layout and §East-at-family-scale.
pub fn compute(
    families: &[FamilyRecord],
    graph: &FamilyGraph,
    config: &LayoutConfig,
) -> LayoutTable {
    todo!()
}
