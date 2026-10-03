// The tide that brings a new wallpaper. A drop falls from Marea and, where it
// lands, the new picture spreads out as a ring of water: the wall of its front
// bends what is under it the most, a line of light rides on top of it, a
// shadow runs ahead of it over the old one, and behind it the water keeps
// rippling until it settles.
//
// It is a group's shader over the NEW picture: it lets through only what the
// wave has already covered. The old one is the real wallpaper, under the
// surface: ahead of the front it is only darkened.
//
// s.a.x  how far the wave has gone, 0..1 (1: the whole monitor)
// s.a.y, s.a.z  where the drop landed, in the group's box
// s.a.w  how settled the water is behind the front, 0..1 (1: still)

fn shade(s: Shader) -> vec4<f32> {
    let progress = clamp(s.a.x, 0.0, 1.0);
    let origin = s.a.yz;
    let p = s.pos;
    let settled = clamp(s.a.w, 0.0, 1.0);
    // How far it has to go to reach the farthest corner.
    let far = length(max(origin, s.size - origin)) + 60.0;
    let r = far * progress;
    let to = p - origin;
    let d = length(to);
    let dir = select(vec2<f32>(0.0, 1.0), to / max(d, 0.001), d > 0.001);

    let back = r - d;
    if (back < -3.0) {
        // Ahead of the front: the shadow the wall of water casts, and a
        // faint first ripple running before it.
        if (progress <= 0.0 || progress >= 1.0) {
            return vec4<f32>(0.0);
        }
        let ahead = -back;
        let dark = 0.34 * exp(-ahead / 30.0);
        let lift = max(sin(ahead * 0.11), 0.0) * 0.06 * exp(-ahead / 60.0);
        let a = dark + lift;
        return vec4<f32>(vec3<f32>(lift / max(a, 0.001)) * vec3<f32>(0.8, 0.95, 1.0), a);
    }
    let edge = smoothstep(-3.0, 5.0, back);
    // Behind the front the water still moves: rings that trail it and fade
    // as they go back, and as the water settles.
    let calm = exp(-max(back, 0.0) / 220.0) * (1.0 - settled);
    let wave = sin(back * 0.075 - s.time * 7.0);
    let off = dir * wave * 11.0 * calm;
    // At the front, the wall of water bends what is under it the most.
    let wall = exp(-pow(max(back, 0.0) / 18.0, 2.0));
    let lens = dir * 14.0 * wall;
    let c = inside(s, clamp(p - off - lens, vec2<f32>(0.0), s.size - vec2<f32>(1.0)));
    var rgb = c.rgb;
    // The line of light on its crest, and a bright shimmer on each ring.
    let crest = exp(-pow((back - 6.0) / 5.0, 2.0));
    rgb = rgb + vec3<f32>(0.85, 0.95, 1.0) * crest * 0.55 * (1.0 - settled);
    rgb = rgb + vec3<f32>(0.6, 0.85, 0.95) * max(wave, 0.0) * 0.07 * calm;
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a * edge);
}
