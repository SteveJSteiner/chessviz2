// Label billboard shader.
// Group 0: camera (shared with other pipelines).
// Group 1: font bitmap storage buffer.
// Instance data carries character codes; the fragment shader renders them
// directly from the 8×8 bitmap font — no screen-space text system needed.

struct Camera {
    view_proj: mat4x4<f32>,
    right:     vec3<f32>,
    _p0:       f32,
    up:        vec3<f32>,
    _p1:       f32,
};
@group(0) @binding(0) var<uniform> cam:  Camera;
@group(1) @binding(0) var<storage, read> font: array<u32>;

// Per-instance label data (64 bytes, matches LabelInstance in Rust).
struct LabelInstance {
    @location(0) center_hw: vec4<f32>,  // xyz = world center, w = half_width
    @location(1) hh_nc:     vec4<f32>,  // x = half_height, y = n_chars (as f32)
    @location(2) chars01:   vec4<u32>,  // ASCII codes for chars 0–3
    @location(3) chars23:   vec4<u32>,  // ASCII codes for chars 4–7
};

struct LabelVsOut {
    @builtin(position)              clip_pos: vec4<f32>,
    @location(0)                    uv:       vec2<f32>,   // -1..1 on the quad
    @location(1) @interpolate(flat) n_chars:  u32,
    @location(2) @interpolate(flat) chars01:  vec4<u32>,
    @location(3) @interpolate(flat) chars23:  vec4<u32>,
};

var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(-1.0, -1.0), vec2( 1.0, -1.0), vec2( 1.0,  1.0),
    vec2(-1.0, -1.0), vec2( 1.0,  1.0), vec2(-1.0,  1.0),
);

@vertex fn vs_label(
    @builtin(vertex_index) vi: u32,
    inst: LabelInstance,
) -> LabelVsOut {
    let center  = inst.center_hw.xyz;
    let half_w  = inst.center_hw.w;
    let half_h  = inst.hh_nc.x;
    let n_chars = u32(inst.hh_nc.y);
    let q       = QUAD[vi];
    // Billboard: expand in camera right/up so the quad always faces the viewer.
    let world   = center + cam.right * (q.x * half_w) + cam.up * (q.y * half_h);
    var out: LabelVsOut;
    out.clip_pos = cam.view_proj * vec4<f32>(world, 1.0);
    out.uv       = q;
    out.n_chars  = n_chars;
    out.chars01  = inst.chars01;
    out.chars23  = inst.chars23;
    return out;
}

@fragment fn fs_label(in: LabelVsOut) -> @location(0) vec4<f32> {
    let bg = vec4<f32>(0.04, 0.04, 0.16, 0.90);

    if in.n_chars == 0u { return bg; }

    // Map quad uv (-1..1) to texture coordinates.
    // tx: 0=left edge, 1=right edge.
    // ty: 0=top edge, 1=bottom edge (flip y so text reads top-to-bottom).
    let tx = 1.0 - (in.uv.x + 1.0) * 0.5;
    let ty = 1.0 - (in.uv.y + 1.0) * 0.5;

    // Thin border: 4% of glyph height on top/bottom, 2% on sides per char.
    let border_y = 0.08;
    let border_x = 0.02 / f32(in.n_chars);
    if ty < border_y || ty > 1.0 - border_y { return bg; }
    if tx < border_x || tx > 1.0 - border_x { return bg; }

    // Which character cell?
    let inner_tx = (tx - border_x) / (1.0 - 2.0 * border_x);
    let inner_ty = (ty - border_y) / (1.0 - 2.0 * border_y);
    let scaled   = inner_tx * f32(in.n_chars);
    let ci       = u32(scaled);
    if ci >= in.n_chars { return bg; }

    // Look up ASCII code from instance data.
    var code: u32;
    if ci < 4u { code = in.chars01[ci]; }
    else        { code = in.chars23[ci - 4u]; }
    code = clamp(code, 32u, 127u);

    // 8×8 bitmap font lookup.
    let cell_u   = fract(scaled);
    let font_col = u32(cell_u   * 8.0);
    let font_row = u32(inner_ty * 8.0);
    let bits     = font[code * 8u + font_row];
    let lit      = (bits >> (7u - font_col)) & 1u;

    if lit == 1u { return vec4<f32>(1.0, 1.0, 0.80, 1.0); }
    return bg;
}
