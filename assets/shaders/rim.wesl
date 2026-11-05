// Characters: the standard PBR vertex stage with loose cloth swayed (cloth.wesl), and the
// fragment plus a rim light, a soft glow along the silhouette (where
// the surface turns away from the eye), stronger on upward-facing edges, so players stand out from
// busy streets. Added as emission before lighting, so it goes through fog and tonemapping like
// everything else. Mirrors bevy_pbr's own pbr.wesl for the forward pass.

import bevy_pbr::render::{
    mesh_functions,
    skinning,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_types::PbrInput,
    forward_io::{Vertex, VertexOutput, FragmentOutput, decompress_vertex},
    view_transformations::position_world_to_clip,
};
import super::cloth::{sway, unpack_velocity};
import bevy_pbr::render::mesh_view_bindings::globals;
import bevy_pbr::render::mesh_view_bindings as view_bindings;
import bevy_pbr::render::mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT;
import bevy_pbr::render::mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT;
import bevy_pbr::render::shadows;

@if(VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;

// rgb: rim colour (linear), a: strength.
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> rim: vec4<f32>;
// Cel shading (the anime characters): x on (> 0.5), y where the light's band falls (N.L), z its
// half-width (how hard the edge is), w the sun kept on the shaded side.
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> toon: vec4<f32>;
// rgb: the shaded side's tint (an anime shadow is a cooler, deeper colour, not grey).
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var<uniform> shade: vec4<f32>;

const PI: f32 = 3.141592653589793;

// Cel lighting: the sun in two flat bands (lit or shaded, its edge hard), its shadow banded the
// same, the ambient under both, the shaded side tinted; the rim's glow on top as emission.
// Point and spot lights are left out (a cel band can't take a soft fill).
fn cel(pbr_input: PbrInput) -> vec4<f32> {
    let albedo = pbr_input.material.base_color.rgb;
    var lit = 0.0;
    var sun = vec3<f32>(0.0);
    if (view_bindings::lights.n_directional_lights > 0u) {
        let light = view_bindings::lights.directional_lights[0];
        let n_dot_l = dot(pbr_input.N, light.direction_to_light);
        var shadow = 1.0;
        if ((pbr_input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (light.flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
            let view_z = dot(vec4<f32>(
                view_bindings::view.view_from_world[0].z,
                view_bindings::view.view_from_world[1].z,
                view_bindings::view.view_from_world[2].z,
                view_bindings::view.view_from_world[3].z
            ), pbr_input.world_position);
            // Looked up 60 cm toward the sun: her own hair, arms and gun (within arm's reach) cast
            // no shade on her (the ponytail's, at the shadow map's resolution, lay on her back as
            // hard squares); trees and buildings, metres off, still shade her.
            let probe = vec4<f32>(pbr_input.world_position.xyz + light.direction_to_light * 0.6, 1.0);
            shadow = shadows::fetch_directional_shadow(0u, probe, pbr_input.world_normal, view_z, pbr_input.frag_coord.xy);
        }
        lit = smoothstep(toon.y - toon.z, toon.y + toon.z, n_dot_l) * smoothstep(0.35, 0.65, shadow);
        sun = light.color.rgb * (1.0 / PI) * mix(toon.w, 1.0, lit);
    }
    let ambient = view_bindings::lights.ambient_color.rgb * mix(shade.rgb, vec3<f32>(1.0), lit);
    let light = albedo * (sun + ambient) * view_bindings::view.exposure;
    let e = pbr_input.material.emissive;
    let emitted = e.rgb * mix(1.0, view_bindings::view.exposure, e.a);
    return vec4<f32>(light + emitted, pbr_input.material.base_color.a);
}

// bevy_pbr's mesh vertex stage (skinned; no morph targets, which the characters don't have), the
// loose cloth swayed in world space after skinning.
@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
@if(SKINNED)
    var world_from_local = skinning::skin_model(vertex.joint_indices, vertex.joint_weights, vertex_in.instance_index);
@else
    var world_from_local = mesh_world_from_local;
@if(VERTEX_NORMALS && SKINNED)
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
@if(VERTEX_NORMALS && !SKINNED)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index);
    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
@if(VERTEX_UVS_B) {
    let motion = vec4<f32>(unpack_velocity(mesh_functions::get_tag(vertex_in.instance_index)), globals.time);
    world = vec4<f32>(sway(world.xyz, vertex.uv_b.x, motion), 1.0);
}
    out.world_position = world;
    out.position = position_world_to_clip(world.xyz);
@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;
@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex_in.instance_index);
@if(VERTEX_COLORS)
    out.color = vertex.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(vertex_in.instance_index, mesh_world_from_local[3]);
    return out;
}

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
    if (toon.x > 0.5) {
        out.color = cel(pbr_input);
    } else {
        out.color = apply_pbr_lighting(pbr_input);
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
