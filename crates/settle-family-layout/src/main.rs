use crude_layout::{compute, LayoutConfig, LayoutTable};
use family_enum::build_table;
use family_graph::build_graph;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let output_path = PathBuf::from(
        std::env::args().nth(1).unwrap_or_else(|| "family_layout.bin".to_owned())
    );

    eprintln!("settle-family-layout");
    eprintln!("  output: {}", output_path.display());

    let t0 = Instant::now();

    eprintln!("  building family table...");
    let families = build_table();

    eprintln!("  building family graph...");
    let graph = build_graph(&families);

    // Long-settling config: more iterations and tighter repulsion than the
    // viewer's interactive default.  Freeze the result into the static artifact.
    let config = LayoutConfig {
        iterations: 400,
        repulsion_distance: 12.0,  // wider search radius for he-scaled repulsion
        anchor_strength: 0.4,      // slightly stronger anchor to keep axis semantics
        max_step: 0.3,
        ..LayoutConfig::default()
    };

    eprintln!(
        "  settling {} families × {} iterations ...",
        families.len(),
        config.iterations
    );
    let layout = compute(&families, &graph, &config);

    let elapsed = t0.elapsed();
    eprintln!("  settled in {:.2}s", elapsed.as_secs_f64());

    print_summary(&layout, &graph);

    eprintln!("  writing {} ...", output_path.display());
    let bytes = bincode::serialize(&layout).expect("serialize LayoutTable");
    let mut f = std::fs::File::create(&output_path).expect("create output file");
    f.write_all(&bytes).expect("write output");

    let hash = {
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in &bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x00000100000001b3);
        }
        h
    };

    eprintln!("  done: {} bytes  fnv64={hash:016x}", bytes.len());
    eprintln!();
    eprintln!("  MANIFEST");
    eprintln!("  families:   {}", layout.layouts.len());
    eprintln!("  iterations: {}", config.iterations);
    eprintln!("  fnv64:      {hash:016x}");
    eprintln!("  elapsed:    {:.2}s", elapsed.as_secs_f64());
}

fn print_summary(layout: &LayoutTable, graph: &family_graph::FamilyGraph) {
    use glam::Vec3;

    let n = layout.layouts.len() as f32;
    let centroid = layout.layouts.iter().map(|fl| fl.center)
        .fold(Vec3::ZERO, |a, c| a + c) / n;

    let mut xs: Vec<f32> = layout.layouts.iter().map(|fl| fl.center.x).collect();
    let mut ys: Vec<f32> = layout.layouts.iter().map(|fl| fl.center.y).collect();
    let mut zs: Vec<f32> = layout.layouts.iter().map(|fl| fl.center.z).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    zs.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut dists: Vec<f32> = graph.edge_indices().map(|e| {
        let (a, b) = graph.edge_endpoints(e).unwrap();
        (layout.layouts[a.index()].center - layout.layouts[b.index()].center).length()
    }).collect();
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let nd = dists.len();
    let diagonal = (Vec3::new(xs[xs.len()-1]-xs[0], ys[ys.len()-1]-ys[0], zs[zs.len()-1]-zs[0])).length();

    eprintln!("  centroid: ({:.1}, {:.1}, {:.1})", centroid.x, centroid.y, centroid.z);
    eprintln!("  east  x: [{:.1}, {:.1}]", xs[0], xs[xs.len()-1]);
    eprintln!("  north y: [{:.1}, {:.1}]", ys[0], ys[ys.len()-1]);
    eprintln!("  radial z:[{:.1}, {:.1}]", zs[0], zs[zs.len()-1]);
    eprintln!("  diagonal: {:.1}", diagonal);
    eprintln!("  edge locality p50/diagonal: {:.3}", dists[nd/2] / diagonal);
}
