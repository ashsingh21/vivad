// Ground: the standard PBR fragment with a tiling detail texture (crate::ground) mixed in by the
// kind of ground under each vertex (second UV: asphalt, slabs; the rest earth). The detail shades
// the colour and, through its screen-space slope, bends the normal (surface-gradient bump mapping),
// so one texture sample gives both. Mirrors bevy_pbr's own pbr.wesl for the forward pass.

import bevy_pbr::render::{
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
};

@if(VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var detail_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var detail_sampler: sampler;
// x: repeats per metre, y: shade strength, z: bump strength.
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var<uniform> detail: vec4<f32>;

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex_output;
@if(VISIBILITY_RANGE_DITHER)
    visibility_range_dither(in.position, in.visibility_range_dither);

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);

    let d = textureSample(detail_texture, detail_sampler, in.world_position.xz * detail.x).rgb;
    var w = vec3<f32>(0.0, 0.0, 1.0);
@if(VERTEX_UVS_B) {
    let a = clamp(in.uv_b, vec2<f32>(0.0), vec2<f32>(1.0));
    w = vec3<f32>(a.x, a.y, max(1.0 - a.x - a.y, 0.0));
}
    let h = dot(d, w) - 0.5;
    pbr_input.material.base_color = vec4<f32>(pbr_input.material.base_color.rgb * (1.0 + 2.0 * detail.y * h), pbr_input.material.base_color.a);

    // The detail as a height field `detail.z` metres deep: tilt the normal down its slope, from the
    // height's and the position's rates of change across the screen (Mikkelsen 2010).
    let height = h * detail.z;
    let dpx = dpdx(in.world_position.xyz);
    let dpy = dpdy(in.world_position.xyz);
    let n = pbr_input.N;
    let r1 = cross(dpy, n);
    let r2 = cross(n, dpx);
    let det = dot(dpx, r1);
    let grad = sign(det) * (dpdx(height) * r1 + dpdy(height) * r2);
    // Seen edge-on the two can cancel to nothing: keep the plain normal there (no NaN).
    let bent = abs(det) * n - grad;
    pbr_input.N = select(n, normalize(bent), dot(bent, bent) > 1e-24);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
