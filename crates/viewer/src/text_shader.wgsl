// Text overlay — no camera transform, screen-space only.

struct GlyphEntry {
    col: u32,
    row: u32,
    code: u32,
    _pad: u32,
}

struct TextParams {
    x0_px:      f32,
    y0_px:      f32,
    char_w_px:  f32,  // column advance (glyph width + gap)
    char_h_px:  f32,  // row advance (glyph height + line gap)
    glyph_w_px: f32,  // rendered pixel width of one glyph
    glyph_h_px: f32,  // rendered pixel height of one glyph
    screen_w:   f32,
    screen_h:   f32,
}

@group(0) @binding(0) var<storage, read> text_font:   array<u32>;
@group(0) @binding(1) var<storage, read> text_glyphs: array<GlyphEntry>;
@group(0) @binding(2) var<uniform>       text_params: TextParams;

struct TextVOut {
    @builtin(position)                  pos:  vec4<f32>,
    @location(0) @interpolate(flat)     code: u32,
    @location(1)                        uv:   vec2<f32>,
}

var<private> QUAD_UV: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
);

@vertex
fn vs_text(
    @builtin(instance_index) inst: u32,
    @builtin(vertex_index)   vert: u32,
) -> TextVOut {
    let g  = text_glyphs[inst];
    let uv = QUAD_UV[vert];

    let px = text_params.x0_px + f32(g.col) * text_params.char_w_px + uv.x * text_params.glyph_w_px;
    let py = text_params.y0_px + f32(g.row) * text_params.char_h_px + uv.y * text_params.glyph_h_px;

    let ndc_x =  px / text_params.screen_w * 2.0 - 1.0;
    let ndc_y =  1.0 - py / text_params.screen_h * 2.0;

    var out: TextVOut;
    out.pos  = vec4(ndc_x, ndc_y, 0.0, 1.0);
    out.code = g.code;
    out.uv   = uv;
    return out;
}

@fragment
fn fs_text(in: TextVOut) -> @location(0) vec4<f32> {
    let row     = u32(in.uv.y * 8.0);
    let col     = u32(in.uv.x * 8.0);
    let code    = clamp(in.code, 32u, 127u);
    let bits    = text_font[code * 8u + row];
    let lit     = (bits >> (7u - col)) & 1u;
    if lit == 0u { discard; }
    return vec4(1.0, 1.0, 0.75, 1.0);  // warm yellow-white
}
