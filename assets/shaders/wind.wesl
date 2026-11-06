// The wind in the trees' leaves (crate::batch: tree and bush leaf materials). Shared by the main
// pass (foliage.wesl) and the prepass (foliage_prepass.wesl), which must move vertices identically
// (depth, shadows and motion vectors match what's drawn).
//
// `p` the vertex in the world, `h` its height over the mesh's origin (a tree's foot, or a merged
// chunk's middle near the ground), `t` seconds, `strength` the material's (0 still .. 1).
fn blow(p: vec3<f32>, h: f32, t: f32, strength: f32) -> vec3<f32> {
    // From the west-north-west, Delhi's hot season wind.
    let dir = vec3<f32>(0.83, 0.0, 0.56);
    // Low canopies barely move, the tops of tall trees most.
    let reach = clamp(h / 7.0, 0.0, 1.4);
    // Gusts rolling across the map, downwind at about 6 m/s: a tree leans, then eases back.
    let along = dot(p.xz, dir.xz);
    let gust = 0.5 + 0.5 * sin(along * 0.045 - t * 0.27 * 6.28) * (0.6 + 0.4 * sin(p.x * 0.011 - p.z * 0.017 + t * 0.21));
    // The canopy nearly as one: a slow sway whose phase drifts smoothly across the map (about a
    // radian over a crown, so neighbouring trees are out of step; never a step, which would
    // tear a mesh).
    let phase = p.x * 0.21 + p.z * 0.17;
    let rock = sin(t * 1.25 + phase) * 0.6 + sin(t * 2.1 + phase * 1.3) * 0.4;
    let lean = (0.05 + 0.13 * gust) * reach;
    let bend = dir * (lean * (0.55 + 0.45 * rock));
    // The leaves' own flutter, quick and small, stronger in a gust.
    let leaf = dot(p, vec3<f32>(3.1, 2.3, 2.7));
    let flutter = vec3<f32>(sin(t * 7.0 + leaf), sin(t * 8.3 + leaf * 1.3) * 0.6, cos(t * 6.1 + leaf * 0.7)) * ((0.010 + 0.022 * gust) * min(reach + 0.3, 1.0));
    // Bent over, the top dips a little.
    let dip = vec3<f32>(0.0, -0.25 * lean * lean, 0.0);
    return p + (bend + flutter + dip) * strength;
}
