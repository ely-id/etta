// Aurora · Etta's wallpapers (BSD-3-Clause). Curtains of the accent colour
// over the base, with a few stars where the curtains are thin.
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
float n(vec2 p) { vec2 i = floor(p); vec2 f = fract(p); f = f * f * (3.0 - 2.0 * f);
  return mix(mix(h(i), h(i + vec2(1.0, 0.0)), f.x), mix(h(i + vec2(0.0, 1.0)), h(i + vec2(1.0, 1.0)), f.x), f.y); }
float fbm(vec2 p) { float v = 0.0; float a = 0.5; for (int i = 0; i < 5; i++) { v += a * n(p); p = p * 2.03 + vec2(1.7, 9.2); a *= 0.5; } return v; }
void main() {
  vec2 uv = v_coords;
  vec2 world = u_camera_center + (uv - 0.5) * u_camera_size;
  vec2 p = world * 0.0011;
  float t = u_time * 0.05;
  float curtain = 0.0;
  for (int k = 0; k < 3; k++) {
    float fk = float(k);
    float x = p.x * 1.3 + fk * 1.7;
    float wave = fbm(vec2(x * 0.8 + t, fk * 3.1 + t * 0.5));
    float band = exp(-pow((uv.y - 0.32 - 0.12 * fk + (wave - 0.5) * 0.35) * 7.0, 2.0));
    float rays = 0.55 + 0.45 * fbm(vec2(x * 6.0, t * 2.0 + fk));
    curtain += band * rays * (0.9 - 0.25 * fk);
  }
  vec3 base = u_base_color * (0.75 + 0.35 * uv.y);
  vec3 col = mix(base, u_accent_color, clamp(curtain * 0.55 * u_intensity, 0.0, 1.0));
  col += mix(u_accent_color, vec3(0.62, 0.84, 0.74), 0.5) * curtain * 0.16 * u_intensity;
  float star = step(0.9965, h(floor(world * 0.35))) * (1.0 - clamp(curtain, 0.0, 1.0));
  col += vec3(star) * 0.6 * u_intensity;
  gl_FragColor = vec4(col * alpha, alpha);
}
