use family_enum::FamilyKey;
use petgraph::graph::Graph;
use petgraph::Undirected;
use serde::{Deserialize, Serialize};

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
        todo!()
    }
}

/// Undirected family transition graph.
/// Node weight: `FamilyKey`. Edge weight: `EdgeMeta`.
pub type FamilyGraph = Graph<FamilyKey, EdgeMeta, Undirected>;

/// Enumerate all family edges by the coarse-delta rules and build the graph.
/// See DESIGN.md §Graph semantics and §Edge taxonomy.
pub fn build_graph(families: &[family_enum::FamilyRecord]) -> FamilyGraph {
    todo!()
}
