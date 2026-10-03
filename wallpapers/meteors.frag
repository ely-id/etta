// Eta Aquariids · Etta's wallpapers (BSD-3-Clause). The meteor shower that
// comet Halley leaves behind every May, and the one Etta is named after:
// streaks fall from a radiant high on the left, over a field of stars.
precision highp float;
//_DEFINES
varying vec2 v_coords;
uniform sampler2D tex;
uniform float alpha;
uniform vec2 u_resolution;
uniform vec2 u_camera_center;
uniform vec2 u_camera_size;
uniform float u_time;
uniform float u_intensity;
uniform vec3 u_base_color;
uniform vec3 u_accent_color;

float hash21(vec2 p) {
  p = fract(p * vec2(0.1031, 0.11369));
  p += dot(p, p.yx + 19.19);
  return fract((p.x + p.y) * p.x);
}
float h(vec2 p) { return hash21(p); }
void main() {
  vec2 uv = v_coords;
  vec2 world = u_camera_center + (uv - 0.5) * u_camera_size;
  float aspect = u_resolution.x / max(u_resolution.y, 1.0);
  vec3 col = mix(u_base_color * 1.25, u_base_color * 0.7, uv.y);
  // The stars, still.
  vec2 cell = floor(world / 7.0);
  float s = step(0.9975, h(cell));
  col += vec3(0.85, 0.9, 1.0) * s * (0.4 + 0.6 * h(cell + 3.1)) * u_intensity;
  // The meteors: each one a short streak leaving the radiant, with its own
  // moment in a cycle of a few seconds.
  vec2 radiant = vec2(0.18, -0.08);
  float meteors = 0.0;
  for (int k = 0; k < 9; k++) {
    float fk = float(k);
    float seed = h(vec2(fk, 7.0));
    float period = 5.0 + seed * 6.0;
    float local = mod(u_time + seed * period, period) / period;
    float ang = 0.35 + seed * 0.9;
    vec2 dir = vec2(cos(ang), sin(ang));
    vec2 start = radiant + dir * (0.15 + 0.25 * h(vec2(fk, 2.0)));
    float travel = local * 1.6;
    vec2 head = start + dir * travel * 0.55;
    vec2 q = (uv - head) * vec2(aspect, 1.0);
    vec2 dq = dir * vec2(aspect, 1.0);
    dq = normalize(dq);
    float along = dot(q, -dq);
    float across = abs(dot(q, vec2(-dq.y, dq.x)));
    float tail = smoothstep(0.16, 0.0, along) * step(0.0, along);
    float thin = smoothstep(0.0022, 0.0, across);
    float alive = smoothstep(0.0, 0.08, local) * smoothstep(0.42, 0.25, local);
    meteors += tail * thin * alive;
  }
  col += mix(u_accent_color, vec3(1.0), 0.35) * meteors * 1.4 * u_intensity;
  col += u_accent_color * 0.05 * exp(-length((uv - radiant) * vec2(aspect, 1.0)) * 2.5) * u_intensity;
  gl_FragColor = vec4(col * alpha, alpha);
}
