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
    forward_io::{Vertex, VertexOutput, FragmentOutput, decompress_vertex},
    view_transformations::position_world_to_clip,
};
import super::cloth::sway;

@if(VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;

// rgb: rim colour (linear), a: strength.
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> rim: vec4<f32>;
// xyz: the warrior's velocity, w: time (crate::rim; cloth.wesl).
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> motion: vec4<f32>;

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
@if(VERTEX_UVS_B)
    world = vec4<f32>(sway(world.xyz, vertex.uv_b.x, motion), 1.0);
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
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
