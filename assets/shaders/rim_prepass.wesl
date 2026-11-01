// The characters' prepass vertex stage (crate::rim): bevy_pbr's prepass vertex (skinned; no
// morph targets) with the loose cloth swayed exactly as the main pass does (cloth.wesl), so depth,
// shadows and motion vectors match what's drawn.

import bevy_pbr::{
    prepass::io::{Vertex, VertexOutput, decompress_vertex},
    render::{mesh_functions, skinning, view_transformations::position_world_to_clip},
};
import super::cloth::sway;

@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> motion: vec4<f32>;

@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
@if(SKINNED)
    var world_from_local = skinning::skin_model(vertex.joint_indices, vertex.joint_weights, vertex_in.instance_index);
@else
    var world_from_local = mesh_world_from_local;
    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
@if(VERTEX_UVS_B)
    world = vec4<f32>(sway(world.xyz, vertex.uv_b.x, motion), 1.0);
    out.world_position = world;
    out.position = position_world_to_clip(world.xyz);
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION) {
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
}
@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS) {
@if(VERTEX_NORMALS && SKINNED)
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
@elif(VERTEX_NORMALS)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index);
@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex_in.instance_index);
}
@if(VERTEX_COLORS)
    out.color = vertex.color;
@if(MOTION_VECTOR_PREPASS) {
    // Last frame's place: the skin's previous pose, swayed the same (the cloth's own motion between
    // frames is small).
@if(SKINNED && HAS_PREVIOUS_SKIN)
    let prev_model = skinning::skin_prev_model(vertex.joint_indices, vertex.joint_weights, vertex_in.instance_index);
@else
    let prev_model = mesh_functions::get_previous_world_from_local(vertex_in.instance_index);
    var prev = mesh_functions::mesh_position_local_to_world(prev_model, vec4<f32>(vertex.position, 1.0));
@if(VERTEX_UVS_B)
    prev = vec4<f32>(sway(prev.xyz, vertex.uv_b.x, motion), 1.0);
    out.previous_world_position = prev;
}
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(vertex_in.instance_index, mesh_world_from_local[3]);
    return out;
}
