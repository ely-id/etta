// Mesh · Etta's wallpapers (BSD-3-Clause). A soft gradient of the base and
// accent colours, its blobs drifting slowly, with a little grain.
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
  vec2 p = (uv - 0.5) * vec2(aspect, 1.0);
  float t = u_time * 0.04;
  vec2 c1 = vec2(-0.55 + 0.12 * sin(t * 1.3), -0.2 + 0.1 * cos(t));
  vec2 c2 = vec2(0.6 + 0.1 * cos(t * 0.9), 0.25 + 0.12 * sin(t * 1.1));
  vec2 c3 = vec2(0.05 + 0.15 * sin(t * 0.7 + 2.0), 0.45 + 0.08 * cos(t * 1.4));
  float w1 = exp(-dot(p - c1, p - c1) * 2.2);
  float w2 = exp(-dot(p - c2, p - c2) * 2.6);
  float w3 = exp(-dot(p - c3, p - c3) * 3.4);
  vec3 col = u_base_color;
  col = mix(col, u_accent_color * 0.55 + u_base_color * 0.45, w1 * 0.75 * u_intensity);
  col = mix(col, u_accent_color.brg * 0.5 + u_base_color * 0.5, w2 * 0.6 * u_intensity);
  col = mix(col, u_accent_color * 0.8, w3 * 0.35 * u_intensity);
  float g = hash21(floor(v_coords * u_resolution));
  col += (g - 0.5) * 0.025;
  gl_FragColor = vec4(col * alpha, alpha);
}
