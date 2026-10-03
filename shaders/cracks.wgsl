// A pane of glass broken by a shot. What glass does: cracks that run out of the
// hole almost straight, zigzagging a little, each one as far as it got;
// straight segments between them, which make the rings polygons; and only the
// middle shattered into shards, each one bending what is behind the screen
// its own way. Past the shattered middle the glass is whole: only the cracks.
//
// s.a.x how far it has cracked (0 to 1) · s.a.y a seed, one per shot ·
// s.a.z how much of it is still there (it fades away at the end)

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn shade(s: Shader) -> vec4<f32> {
    let grow = s.a.x;
    let seed = s.a.y;
    let fade = s.a.z;
    let centre = s.size * 0.5;
    let d = s.pos - centre;
    let r = length(d);
    let reach = min(centre.x, centre.y) * grow;
    if (fade <= 0.0 || r > reach + 2.0) {
        return vec4<f32>(0.0);
    }
    let angle = atan2(d.y, d.x);

    // The cracks: 9 to 13, almost straight, with a small zigzag.
    let rays = 9.0 + floor(hash2(vec2<f32>(seed, 1.0)) * 5.0);
    let zigzag = 0.05 * sin(r * 0.31 + seed * 2.0) + 0.035 * sin(r * 0.83 + seed * 5.0);
    let u = (angle / 6.2831853 + 0.5) * rays + zigzag;
    let sector = floor(u);
    let f = fract(u);
    let ray_distance = min(f, 1.0 - f) / rays * 6.2831853 * r;
    // Each crack as far as it got.
    let ray = sector + step(0.5, f);
    let length_of = reach * (0.35 + 0.65 * hash2(vec2<f32>(ray, seed)));
    let ray_line = (1.0 - smoothstep(0.3, 1.15, ray_distance)) * (1.0 - smoothstep(length_of * 0.8, length_of, r));

    // The segments between two cracks: straight, so the rings come out as
    // polygons. Measured along the sector's middle line.
    let middle = (sector + 0.5) / rays * 6.2831853 - 3.14159265;
    let along = r * cos(angle - middle);
    let spacing = reach * (0.18 + 0.12 * hash2(vec2<f32>(sector, seed + 3.0)));
    let q = along / max(spacing, 1.0) + 0.25 * hash2(vec2<f32>(sector + 7.0, seed));
    let ring = floor(q);
    let ring_distance = min(fract(q), 1.0 - fract(q)) * spacing;
    let has_ring = step(0.3, hash2(vec2<f32>(sector, ring) + seed + 3.3)) * step(1.0, ring) * step(ring, 2.0);
    let ring_line = (1.0 - smoothstep(0.3, 1.0, ring_distance)) * has_ring * 0.7 * step(r, length_of);
    let crack = max(ray_line, ring_line);

    // The shattered middle: the first polygon, and the second one a little.
    let shattered = 1.0 - step(1.0, q);
    let loose = shattered + 0.4 * (1.0 - shattered) * (1.0 - step(2.0, q));
    let cell = vec2<f32>(sector, ring);
    let tilt = vec2<f32>(hash2(cell + seed), hash2(cell.yx + seed * 1.7)) - 0.5;
    let seen = behind(s, s.pos + tilt * 8.0 * loose);
    // Each shard catches the light its own way; the whole glass does not.
    let colour0 = mix(seen.rgb, vec3<f32>(1.0), (0.04 + 0.12 * (tilt.x + 0.5)) * loose);
    var colour = mix(colour0, vec3<f32>(0.93, 0.97, 1.0), crack);

    // The hole: dark, torn, with a bright rim of crushed glass.
    let torn = 5.0 + 1.8 * sin(angle * 7.0 + seed * 5.0) + 0.8 * sin(angle * 13.0 + seed);
    let hole = 1.0 - smoothstep(torn - 0.8, torn + 0.8, r);
    let rim = (1.0 - smoothstep(0.0, 3.0, abs(r - torn - 2.5))) * 0.8;
    colour = mix(colour, vec3<f32>(1.0), rim * (1.0 - hole));
    colour = mix(colour, vec3<f32>(0.03, 0.03, 0.035), hole);

    // What is behind only where the glass is loose, with the lens's alpha so
    // the next capture still sees through; the cracks and the hole, always.
    let a = max(max(crack, hole), max(rim, seen.a * 0.88 * step(0.01, loose))) * fade;
    return vec4<f32>(colour, a);
}
