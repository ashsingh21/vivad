// Cloth that moves (crate::rim): how far a vertex sways, from how loose it is there (`flex`, the
// vertex colour's alpha, 0 fastened .. 1 free hem) and the warrior's motion (xyz velocity m/s,
// w time s). Shared by the main pass (rim.wesl) and the prepass (rim_prepass.wesl), which must move
// vertices identically.

fn sway(p: vec3<f32>, flex: f32, motion: vec4<f32>) -> vec3<f32> {
    let v = motion.xyz;
    let t = motion.w;
    let ground = vec3<f32>(v.x, 0.0, v.z);
    let speed = min(length(ground), 10.0);
    let back = select(vec3<f32>(0.0), -ground / max(length(ground), 1e-3), speed > 0.05);
    let side = vec3<f32>(-back.z, 0.0, back.x);
    // Trails behind as the warrior moves, lifting with speed; blown up when falling.
    let fall = clamp(-v.y, 0.0, 45.0);
    let trail = back * (0.022 * speed) + vec3<f32>(0.0, 0.009 * speed + 0.006 * fall, 0.0);
    // A flutter that quickens and grows with speed (and in freefall); a breath when standing.
    let rate = 3.0 + 1.1 * speed + 0.25 * fall;
    let wave = sin(t * rate + p.y * 9.0 + p.x * 4.0) * 0.6 + sin(t * rate * 1.7 + p.z * 7.0) * 0.4;
    let amp = 0.008 + 0.006 * speed + 0.004 * fall;
    let flutter = (side * 0.7 + back * 0.3 + vec3<f32>(0.0, 0.3, 0.0)) * wave * amp;
    return p + (trail + flutter) * flex;
}
