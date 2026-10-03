// Ripples · Etta's wallpapers (BSD-3-Clause). Rings spreading on still
// water from a few places, the way she lands.
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
void main() {
  vec2 uv = v_coords;
  float aspect = u_resolution.x / max(u_resolution.y, 1.0);
  vec2 p = uv * vec2(aspect, 1.0);
  vec3 col = mix(u_base_color * 1.15, u_base_color * 0.8, uv.y);
  float rings = 0.0;
  for (int k = 0; k < 4; k++) {
    float fk = float(k);
    vec2 c = vec2(hash21(vec2(fk, 1.0)) * aspect, 0.2 + hash21(vec2(fk, 5.0)) * 0.6);
    float period = 7.0 + fk * 1.7;
    float age = mod(u_time + fk * 2.3, period) / period;
    float d = length(p - c);
    for (int j = 0; j < 3; j++) {
      float r = age * 0.55 - float(j) * 0.05;
      float ring = smoothstep(0.004, 0.0, abs(d - r)) * step(0.0, r);
      rings += ring * (1.0 - age) * (1.0 - float(j) * 0.3);
    }
  }
  col += u_accent_color * rings * 0.5 * u_intensity;
  gl_FragColor = vec4(col * alpha, alpha);
}
