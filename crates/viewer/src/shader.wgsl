// Shared camera uniform (bind group 0, binding 0)
struct Camera {
    view_proj: mat4x4<f32>,
    right: vec3<f32>,
    _p0: f32,
    up: vec3<f32>,
    _p1: f32,
};
@group(0) @binding(0) var<uniform> cam: Camera;

// ── Family billboards ─────────────────────────────────────────────────────────

struct FamilyInstance {
    @location(0) center_size: vec4<f32>,   // xyz = world center, w = half-size
    @location(1) color: vec4<f32>,
};

struct FamilyVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,            // quad-local coords in [-1, 1]
};

var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0,  1.0),
);

@vertex fn vs_family(
    @builtin(vertex_index) vi: u32,
    inst: FamilyInstance,
) -> FamilyVsOut {
    let center = inst.center_size.xyz;
    let half_size = inst.center_size.w;
    let q = QUAD[vi];
    let world = center + cam.right * (q.x * half_size) + cam.up * (q.y * half_size);
    var out: FamilyVsOut;
    out.clip_pos = cam.view_proj * vec4<f32>(world, 1.0);
    out.color = inst.color;
    out.uv = q;
    return out;
}

@fragment fn fs_family(in: FamilyVsOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    // Discard corners — render as a circle.
    if d > 1.0 { discard; }
    // Soft edge in the outer 20% of the radius.
    let alpha = in.color.a * (1.0 - smoothstep(0.8, 1.0, d));
    return vec4<f32>(in.color.rgb, alpha);
}

// ── Edges ─────────────────────────────────────────────────────────────────────

struct EdgeVertex {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct EdgeVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex fn vs_edge(v: EdgeVertex) -> EdgeVsOut {
    var out: EdgeVsOut;
    out.clip_pos = cam.view_proj * vec4<f32>(v.pos, 1.0);
    out.color = v.color;
    return out;
}

@fragment fn fs_edge(in: EdgeVsOut) -> @location(0) vec4<f32> {
    return in.color;
}
