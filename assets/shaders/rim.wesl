// Characters: the standard PBR fragment plus a rim light, a soft glow along the silhouette (where
// the surface turns away from the eye), stronger on upward-facing edges, so players stand out from
// busy streets. Added as emission before lighting, so it goes through fog and tonemapping like
// everything else. Mirrors bevy_pbr's own pbr.wesl for the forward pass.

import bevy_pbr::render::{
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
};

@if(VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;

// rgb: rim colour (linear), a: strength.
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> rim: vec4<f32>;

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex_output;
@if(VISIBILITY_RANGE_DITHER)
    visibility_range_dither(in.position, in.visibility_range_dither);

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);

    let facing = clamp(dot(pbr_input.N, pbr_input.V), 0.0, 1.0);
    let edge = pow(1.0 - facing, 3.0);
    let lift = 0.55 + 0.45 * clamp(pbr_input.N.y, 0.0, 1.0);
    let glow = rim.rgb * (rim.a * edge * lift);
    pbr_input.material.emissive = vec4<f32>(pbr_input.material.emissive.rgb + glow, pbr_input.material.emissive.a);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
