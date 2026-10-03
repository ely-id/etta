// Dusk · Etta's wallpapers (BSD-3-Clause). A low sun over a soft hill,
// its sky going from the base colour to the accent.
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
  vec2 a = (uv - vec2(0.5, 0.62)) * vec2(u_resolution.x / max(u_resolution.y, 1.0), 1.0);
  float sun = smoothstep(0.17, 0.16, length(a));
  float glow = exp(-length(a) * 3.5);
  vec3 top = u_base_color * 0.9;
  vec3 hor = mix(u_accent_color, vec3(0.95, 0.55, 0.4), 0.55);
  vec3 col = mix(top, hor, smoothstep(0.1, 0.85, uv.y));
  col += hor * glow * 0.35 * u_intensity + vec3(1.0, 0.85, 0.6) * sun * 0.6 * u_intensity;
  float hill = smoothstep(0.0, 0.004, uv.y - (0.72 + 0.03 * sin(uv.x * 9.0) + 0.02 * sin(uv.x * 23.0 + 1.0)));
  col = mix(col, u_base_color * 0.45, hill);
  float g = hash21(floor(v_coords * u_resolution));
  col += (g - 0.5) * 0.03;
  gl_FragColor = vec4(col * alpha, alpha);
}
