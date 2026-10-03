// Tide · Etta's wallpapers (BSD-3-Clause). Lines of water drifting at
// different depths, each a shade deeper than the one above.
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

void main() {
  vec2 uv = v_coords;
  vec2 world = u_camera_center + (uv - 0.5) * u_camera_size;
  float t = u_time * 0.25;
  vec3 col = mix(u_base_color * 1.2, u_base_color * 0.75, uv.y);
  float lines = 0.0;
  for (int k = 0; k < 6; k++) {
    float fk = float(k);
    float y = 0.2 + fk * 0.13;
    float w = sin(world.x * 0.004 + fk * 1.3 + t * (0.6 + 0.1 * fk)) * 0.016 + sin(world.x * 0.0013 - fk + t * 0.3) * 0.03;
    float d = abs(uv.y - y - w);
    lines += smoothstep(0.0035, 0.0, d) * (0.5 - fk * 0.06);
    col = mix(col, u_accent_color * 0.3 + u_base_color * 0.7, step(0.0, uv.y - y - w) * 0.07);
  }
  col += u_accent_color * lines * 0.55 * u_intensity;
  gl_FragColor = vec4(col * alpha, alpha);
}
