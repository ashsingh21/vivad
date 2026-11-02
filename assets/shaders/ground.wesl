// The ground on a city map: the standard PBR fragment, with its surface swapped before lighting
// (same frame as rim.wesl). Which surface shows at a pixel comes from a mask of the map's ground
// classes (crate::surfaces). The terrain has no tangents; on ground this flat they follow from the
// normal and world X.

import bevy_pbr::render::{
    pbr_bindings::base_color_sampler,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
};

@if(VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> place: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var mask: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var asphalt_color: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var asphalt_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var asphalt_orm: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var pavers_color: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var pavers_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(107) var pavers_orm: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(108) var soil_color: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(109) var soil_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(110) var soil_orm: texture_2d<f32>;

// Metres per repeat (tools/textures/cp_surfaces.py).
const ASPHALT_TILE: f32 = 12.0;
const PAVERS_TILE: f32 = 4.0;
const SOIL_TILE: f32 = 6.0;

// Linear colour (as a ratio, 0.5 meaning as painted), normal XY, occlusion and roughness, summed
// over the surfaces under a pixel.
struct Layers {
    color: vec3<f32>,
    normal: vec2<f32>,
    occlusion: f32,
    roughness: f32,
}

fn add_surface(sum: ptr<function, Layers>, weight: f32, color: texture_2d<f32>, normal: texture_2d<f32>, orm: texture_2d<f32>, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) {
    let c = textureSampleGrad(color, base_color_sampler, uv, dx, dy).rgb;
    let n = textureSampleGrad(normal, base_color_sampler, uv, dx, dy).rg * 2.0 - 1.0;
    let m = textureSampleGrad(orm, base_color_sampler, uv, dx, dy);
    (*sum).color += c * weight;
    (*sum).normal += n * weight;
    (*sum).occlusion += m.r * weight;
    (*sum).roughness += m.g * weight;
}

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex_output;
@if(VISIBILITY_RANGE_DITHER)
    visibility_range_dither(in.position, in.visibility_range_dither);

    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let p = in.world_position.xz;
    let dx = dpdx(p);
    let dy = dpdy(p);
    // The mask is about a metre a texel: a wandering threshold makes its edges a kerb's irregular
    // line rather than a blur.
    let wander = sin(p.x * 1.7 + 1.5 * sin(p.y * 2.3)) * sin(p.y * 1.9 + 1.5 * sin(p.x * 1.1));
    let share = textureSampleLevel(mask, base_color_sampler, (p + place.x) * place.y + place.z, 0.0).rgb;
    var w = smoothstep(vec3<f32>(0.40), vec3<f32>(0.60), share + 0.1 * wander);
    let total = w.x + w.y + w.z;
    if (total < 0.001) {
        w = share / max(share.x + share.y + share.z, 0.001);
    } else {
        w /= total;
    }

    // Only what's showing is read: most pixels are one surface, and pay for three maps, not nine.
    var sum = Layers(vec3<f32>(0.0), vec2<f32>(0.0), 0.0, 0.0);
    if (w.x > 0.004) {
        add_surface(&sum, w.x, asphalt_color, asphalt_normal, asphalt_orm, p / ASPHALT_TILE, dx / ASPHALT_TILE, dy / ASPHALT_TILE);
    }
    if (w.y > 0.004) {
        add_surface(&sum, w.y, pavers_color, pavers_normal, pavers_orm, p / PAVERS_TILE, dx / PAVERS_TILE, dy / PAVERS_TILE);
    }
    if (w.z > 0.004) {
        add_surface(&sum, w.z, soil_color, soil_normal, soil_orm, p / SOIL_TILE, dx / SOIL_TILE, dy / SOIL_TILE);
    }

    // The painted colour and baked contact shadow stay; the maps add the grain.
    let base = pbr_input.material.base_color;
    pbr_input.material.base_color = vec4<f32>(base.rgb * sum.color * 2.0, base.a);
    pbr_input.material.perceptual_roughness = sum.roughness;
    pbr_input.diffuse_occlusion *= sum.occlusion;

    let up = pbr_input.world_normal;
    let t = normalize(vec3<f32>(1.0, 0.0, 0.0) - up * up.x);
    let b = cross(up, t);
    let z = sqrt(saturate(1.0 - dot(sum.normal, sum.normal)));
    pbr_input.N = normalize(t * sum.normal.x + b * sum.normal.y + up * z);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
