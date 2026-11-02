// The ground on a city map: the standard PBR fragment, with its surface swapped before lighting
// (same frame as rim.wesl). Which surface shows at a pixel comes from a mask of the map's ground
// (crate::surfaces): its r, g and b are metres inside asphalt, pavers and land, so an edge can be
// as sharp as a pixel however coarse the terrain's grid, and a kerb has a width in metres. The
// terrain has no tangents; on ground this flat they follow from the normal and world X.

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
@group(constants::MATERIAL_BIND_GROUP) @binding(108) var dry_color: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(109) var dry_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(110) var dry_orm: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(111) var turf_color: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(112) var turf_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(113) var turf_orm: texture_2d<f32>;

// Metres per repeat (tools/textures/cp_surfaces.py, ncr_ground.py).
const ASPHALT_TILE: f32 = 12.0;
const PAVERS_TILE: f32 = 4.0;
const DRY_TILE: f32 = 3.2;
const TURF_TILE: f32 = 2.7;
// How far the mask's distances run, metres (crate::surfaces).
const REACH: f32 = 6.0;

// Each surface's own colour, linear: on a big map the base colour is only a tint of these
// (crate::surfaces::surface_mean has them in sRGB; here they can be tuned without a rebuild).
const ASPHALT_MEAN: vec3<f32> = vec3<f32>(0.0783, 0.0757, 0.0757);
const PAVERS_MEAN: vec3<f32> = vec3<f32>(0.2429, 0.2187, 0.1831);
const DRY_MEAN: vec3<f32> = vec3<f32>(0.2530, 0.1960, 0.1128);
const TURF_MEAN: vec3<f32> = vec3<f32>(0.0783, 0.1065, 0.0301);
const BARE_MEAN: vec3<f32> = vec3<f32>(0.2429, 0.2049, 0.1473);
const KERB_MEAN: vec3<f32> = vec3<f32>(0.3185, 0.3013, 0.2633);
const DUST_MEAN: vec3<f32> = vec3<f32>(0.2140, 0.1789, 0.1329);

// How much each surface reflects, Bevy's 0 to 1 (0.5 is 4 %). Grass and dry earth hardly do: at the
// default a low sun laid a milky sheen over the land.
const ASPHALT_SHINE: f32 = 0.38;
const PAVERS_SHINE: f32 = 0.42;
const LAND_SHINE: f32 = 0.16;

// Across a kerb from the road, metres: its face, then its top.
const KERB_FACE: f32 = 0.09;
const KERB_TOP: f32 = 0.26;

// Linear colour (as a ratio, 0.5 meaning as the mean), normal XY, occlusion and roughness, summed
// over the surfaces under a pixel.
struct Layers {
    color: vec3<f32>,
    normal: vec2<f32>,
    occlusion: f32,
    roughness: f32,
}

fn tiled(map: texture_2d<f32>, p: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, tile: f32) -> vec4<f32> {
    return textureSampleGrad(map, base_color_sampler, p / tile, dx / tile, dy / tile);
}

// `detail` under 1 quietens a scan: bare earth is the dry grass with its blades mostly gone.
fn add_surface(sum: ptr<function, Layers>, weight: f32, mean: vec3<f32>, detail: f32, color: texture_2d<f32>, normal: texture_2d<f32>, orm: vec4<f32>, p: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, tile: f32) {
    let c = tiled(color, p, dx, dy, tile).rgb;
    let n = tiled(normal, p, dx, dy, tile).rg * 2.0 - 1.0;
    (*sum).color += (vec3<f32>(0.5) + (c - vec3<f32>(0.5)) * detail) * mean * weight;
    (*sum).normal += n * detail * weight;
    (*sum).occlusion += (1.0 + (orm.r - 1.0) * detail) * weight;
    (*sum).roughness += orm.g * weight;
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
    // Metres a pixel covers here, along the worse axis.
    let foot = max(length(dx), length(dy));
    let mask_uv = (p + place.x) * place.y + place.z;
    let m = textureSampleLevel(mask, base_color_sampler, mask_uv, 0.0);

    // A big map's colours are this code's; a small map's are painted.
    let own = place.w;
    let mean = mix(vec3<f32>(1.0), ASPHALT_MEAN, own);

    // Metres inside each surface. Land's edges wander; kerbs run straight.
    var d = (m.rgb - vec3<f32>(0.5)) * 2.0 * REACH;
    let wander = sin(p.x * 1.7 + 1.5 * sin(p.y * 2.3)) * sin(p.y * 1.9 + 1.5 * sin(p.x * 1.1));
    d.z += 0.35 * wander;
    let top = max(d.x, max(d.y, d.z));
    var w = exp(clamp((d - vec3<f32>(top)) / max(0.04, foot * 0.8), vec3<f32>(-12.0), vec3<f32>(0.0)));
    w /= w.x + w.y + w.z;

    // Only what's showing is read: most pixels are one surface, and pay for three maps, not twelve.
    var sum = Layers(vec3<f32>(0.0), vec2<f32>(0.0), 0.0, 0.0);
    if (w.x > 0.004) {
        add_surface(&sum, w.x, mean, 1.0, asphalt_color, asphalt_normal, tiled(asphalt_orm, p, dx, dy, ASPHALT_TILE), p, dx, dy, ASPHALT_TILE);
    }
    if (w.y > 0.004) {
        add_surface(&sum, w.y, mix(vec3<f32>(1.0), PAVERS_MEAN, own), 1.0, pavers_color, pavers_normal, tiled(pavers_orm, p, dx, dy, PAVERS_TILE), p, dx, dy, PAVERS_TILE);
    }
    if (w.z > 0.004) {
        // The mask's alpha says how green the land is. Noise at two sizes breaks the line (the dry
        // scan's blue, here and turned on a long tile), and it softens with distance: from a
        // rooftop this is a change of colour, not a pattern of flecks.
        let orm = tiled(dry_orm, p, dx, dy, DRY_TILE);
        let broad = tiled(dry_orm, p.yx + vec2<f32>(5.3, 1.7), dy.yx, dx.yx, 29.0).b;
        let green = saturate(m.a * 2.0 - 1.0);
        let bare = saturate(1.0 - m.a * 2.0);
        let spread = 0.12 + 0.38 * smoothstep(0.01, 0.12, foot);
        let t = smoothstep(0.5 - spread, 0.5 + spread, green * 1.5 - 0.25 + (orm.b - 0.5) * 0.35 + (broad - 0.5) * 0.65);
        if (t < 0.996) {
            add_surface(&sum, w.z * (1.0 - t), mix(vec3<f32>(1.0), mix(DRY_MEAN, BARE_MEAN, bare), own), 1.0 - 0.65 * bare, dry_color, dry_normal, orm, p, dx, dy, DRY_TILE);
        }
        if (t > 0.004) {
            add_surface(&sum, w.z * t, mix(vec3<f32>(1.0), TURF_MEAN, own), 1.0, turf_color, turf_normal, tiled(turf_orm, p, dx, dy, TURF_TILE), p, dx, dy, TURF_TILE);
        }
    }

    // Where a road meets a pavement: dust along the edge, a dark gutter, and a kerb drawn from the
    // mask, with no geometry. `e` is metres inside the road.
    let e = d.x;
    if (e > -KERB_TOP - foot - 0.02 && e < 3.2) {
        let at_pavement = smoothstep(-2.6, -1.4, d.y);
        let aa = max(foot * 0.8, 0.012);
        let face = at_pavement * (smoothstep(-KERB_FACE - aa, -KERB_FACE + aa, e) - smoothstep(-aa, aa, e));
        let cap = at_pavement * (smoothstep(-KERB_TOP - aa, -KERB_TOP + aa, e) - smoothstep(-KERB_FACE - aa, -KERB_FACE + aa, e));
        // Dust along every road's edge, thicker in drifts; the gutter only under a kerb.
        let inside = saturate(e / max(aa, 0.03));
        sum.color *= 1.0 - 0.42 * inside * at_pavement * (1.0 - smoothstep(0.02, 0.5, e));
        let dust = inside * (1.0 - smoothstep(0.1, 3.0, e)) * (0.5 + 0.34 * wander);
        sum.color = mix(sum.color, mix(sum.color, 0.5 * DUST_MEAN, own), saturate(dust));
        // A stone every 75 cm or so, each its own shade.
        let stone = fract(sin(dot(floor(p / 0.75), vec2<f32>(12.9898, 78.233))) * 43758.5453);
        let kerb = mix(sum.color, 0.5 * KERB_MEAN * (0.84 + 0.3 * stone), own);
        sum.color = mix(sum.color, kerb, saturate(face + cap));
        sum.roughness = mix(sum.roughness, 0.85, saturate(face + cap));
        if (face > 0.004) {
            // The face looks at the road: down the mask's slope.
            let h = place.y;
            let gx = textureSampleLevel(mask, base_color_sampler, mask_uv + vec2<f32>(h, 0.0), 0.0).r - textureSampleLevel(mask, base_color_sampler, mask_uv - vec2<f32>(h, 0.0), 0.0).r;
            let gz = textureSampleLevel(mask, base_color_sampler, mask_uv + vec2<f32>(0.0, h), 0.0).r - textureSampleLevel(mask, base_color_sampler, mask_uv - vec2<f32>(0.0, h), 0.0).r;
            let slope = normalize(vec2<f32>(gx, gz) + vec2<f32>(1e-5, 0.0));
            sum.normal = mix(sum.normal, vec2<f32>(slope.x, -slope.y) * 0.82, face);
            sum.occlusion *= 1.0 - 0.3 * face;
        }
    }

    // The tint, or the painted colour and baked contact shadow: the maps add the grain.
    let base = pbr_input.material.base_color;
    pbr_input.material.base_color = vec4<f32>(base.rgb * mix(1.0, 2.0, own) * sum.color * 2.0, base.a);
    pbr_input.material.perceptual_roughness = sum.roughness;
    pbr_input.material.reflectance = vec3<f32>(w.x * ASPHALT_SHINE + w.y * PAVERS_SHINE + w.z * LAND_SHINE);
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
