// A reservoir: a glass tube with water in it, as full as the window has left.
// The surface is never flat —two slow waves and whatever the tube is doing—,
// light plays inside it, bubbles rise to the top and burst there, and the
// mouse over it stirs the water where it passes.
//
// s.a.x how full it is (0 to 1; below 0, unknown: an empty tube with a dashed
//       line where the water would be) · s.a.y how much it sways (a spring:
//       it tilts the surface) · s.a.z a seed, one per tube · s.a.w pending (the
//       window renewed and nobody has confirmed it yet: the water breathes)
// s.b.x how lit it is (hover) · s.b.y how alarmed (little left: it glows)
// s.color the water · s.color2 the deep of it

fn hash1(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn shade(s: Shader) -> vec4<f32> {
    let level = s.a.x;
    let sway = s.a.y;
    let seed = s.a.z;
    let pending = s.a.w;
    let lit = s.b.x;
    let alarm = s.b.y;
    let t = s.time;
    let p = s.pos;
    let w = s.size.x;
    let h = s.size.y;
    let pad = 4.0;

    // The tube: dark glass, a line of light down its left side and another,
    // thinner, on the right. It is always there, full or empty.
    var col = vec3<f32>(0.09, 0.10, 0.11);
    var a = 0.62 + 0.1 * lit;
    let sheen = exp(-pow((p.x - w * 0.2) / 2.2, 2.0)) * 0.18 + exp(-pow((p.x - w * 0.84) / 1.2, 2.0)) * 0.08;

    // Marks every quarter on the right, like a measuring glass.
    let inner = h - 2.0 * pad;
    var marks = 0.0;
    for (var k = 1; k < 4; k++) {
        let my = pad + inner * f32(k) / 4.0;
        marks += (1.0 - smoothstep(0.4, 1.1, abs(p.y - my))) * step(w - 12.0, p.x) * step(p.x, w - 5.0);
    }

    if (level < 0.0) {
        // Unknown: no water at all, and a dashed line at half height saying
        // «something should be here».
        let dash = step(0.5, fract(p.x / 6.0)) * (1.0 - smoothstep(0.3, 1.0, abs(p.y - h * 0.5)));
        col = col + vec3<f32>(sheen) + vec3<f32>(0.35) * dash + vec3<f32>(0.25) * marks;
        return vec4<f32>(col, a);
    }

    // The surface: where the level says, moved by two slow waves, tilted by
    // the sway, and stirred where the mouse is.
    let full = clamp(level, 0.0, 1.0);
    let breathe = pending * 0.02 * sin(t * 1.6);
    var surface = pad + inner * (1.0 - full - breathe);
    surface += 1.6 * sin(p.x * 0.23 + t * 2.1 + seed * 3.0) + 1.1 * sin(p.x * 0.47 - t * 3.3 + seed);
    surface += sway * (p.x - w * 0.5) * 0.35;
    let stir = s.hovered * exp(-abs(p.x - s.pointer.x) / 9.0) * 2.4 * sin(t * 9.0 - abs(p.x - s.pointer.x) * 0.6);
    surface += stir;

    let under = smoothstep(surface - 0.6, surface + 0.6, p.y) * step(p.y, h - pad + 1.0);
    if (under > 0.0) {
        // Deeper is darker; near the top the light gets in.
        let depth = clamp((p.y - surface) / max(h - surface, 1.0), 0.0, 1.0);
        var water = mix(s.color.rgb, s.color2.rgb, pow(depth, 0.8));
        // The light playing inside: bands that bend and slide.
        let c1 = sin(p.x * 0.21 + t * 0.9 + sin(p.y * 0.11 - t * 0.6 + seed) * 1.2);
        let c2 = sin(p.x * 0.13 - p.y * 0.09 - t * 0.7 + seed * 2.0);
        let caustic = pow(0.5 + 0.25 * (c1 + c2), 3.0);
        water += vec3<f32>(caustic * 0.14 * (1.0 - depth * 0.7));
        // Bubbles: each one in its column, rising at its own pace, wobbling a
        // little, gone when it reaches the top.
        for (var i = 0; i < 6; i++) {
            let fi = f32(i);
            let bx = pad + 3.0 + hash1(vec2<f32>(fi, seed)) * (w - 2.0 * pad - 6.0);
            let speed = 0.18 + 0.22 * hash1(vec2<f32>(seed, fi + 4.0));
            let phase = fract(t * speed + hash1(vec2<f32>(fi * 3.1, seed + 1.0)));
            let by = h - pad - phase * (h - pad - surface);
            let r = 1.1 + 1.4 * hash1(vec2<f32>(fi + 9.0, seed));
            let cx = bx + 1.4 * sin(t * 3.0 + fi * 2.0);
            let d = length(vec2<f32>(p.x - cx, p.y - by));
            let ring = (1.0 - smoothstep(r - 0.5, r + 0.5, d)) * (0.35 + 0.65 * smoothstep(r - 1.2, r, d));
            water += vec3<f32>(ring * 0.45 * (1.0 - smoothstep(0.85, 1.0, phase)));
        }
        // With little left, the water glows from inside, slowly.
        water += s.color.rgb * alarm * (0.25 + 0.25 * sin(t * 3.2));
        col = mix(col, water, under);
        a = mix(a, 0.96, under);
    }
    // The meniscus: a bright line where water meets air.
    let edge = 1.0 - smoothstep(0.0, 1.4, abs(p.y - surface));
    col += vec3<f32>(0.85, 0.95, 1.0) * edge * 0.7 * step(p.y, h - pad);
    col += vec3<f32>(sheen) + vec3<f32>(0.3) * marks * (1.0 - under * 0.5);
    col += vec3<f32>(lit * 0.05);
    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), clamp(a + edge * 0.3, 0.0, 1.0));
}
