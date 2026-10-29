// Grass blades, depth / normal / motion-vector prepass: the same displacement as grass.wesl,
// and last frame's for motion vectors (so TAA doesn't smear swaying blades).

import bevy_pbr::{
    prepass::io::{Vertex, VertexOutput, decompress_vertex},
    render::{
        mesh_functions,
        mesh_view_bindings::view,
        view_transformations::position_world_to_clip,
    },
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> wind: vec4<f32>;

const NEAR: f32 = 13.0;
const FAR: f32 = 40.0;

fn displace(p: vec3<f32>, f: f32, rank: f32, height: f32, t: f32, eye: vec3<f32>) -> vec3<f32> {
    let dir = vec2<f32>(0.94, 0.34);
    let gust = 0.5 + 0.5 * sin(dot(p.xz, dir) * 0.21 - t * 1.7);
    let flutter = sin(p.x * 2.3 + p.z * 1.9 + t * 5.1) * 0.35 + sin(p.x * 0.9 - p.z * 2.7 + t * 3.7) * 0.25;
    let push = dir * (0.15 + 0.85 * gust * gust + 0.25 * flutter) + vec2<f32>(-dir.y, dir.x) * (0.3 * flutter);
    let k = f * f * wind.y * min(height / 0.35, 1.4);
    var q = p + vec3<f32>(push.x * k, -0.35 * k * length(push) * f, push.y * k);
    let reach = mix(FAR, NEAR, rank);
    // Also clear blades out of the camera's face when it dips into them.
    let s = max(smoothstep(reach - 7.0, reach, distance(p.xz, eye.xz)), 1.0 - smoothstep(0.8, 1.6, distance(p, eye)));
    q.y -= s * (f * height + 0.04);
    return q;
}

@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let v = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
    let p = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.world_position = vec4<f32>(displace(p.xyz, v.uv.y, v.uv.x, v.color.a, wind.x, view.world_position), 1.0);
    out.position = position_world_to_clip(out.world_position.xyz);
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION) {
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
}
    out.uv = v.uv;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, vertex_in.instance_index);
@if(VERTEX_COLORS)
    out.color = vec4<f32>(v.color.rgb, 1.0);
@if(MOTION_VECTOR_PREPASS) {
    let prev_model = mesh_functions::get_previous_world_from_local(vertex_in.instance_index);
    let pp = mesh_functions::mesh_position_local_to_world(prev_model, vec4<f32>(v.position, 1.0));
    out.previous_world_position = vec4<f32>(displace(pp.xyz, v.uv.y, v.uv.x, v.color.a, wind.z, view.world_position), 1.0);
}
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(vertex_in.instance_index, world_from_local[3]);
    return out;
}
