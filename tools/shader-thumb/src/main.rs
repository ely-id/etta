//! A Halley field shader, painted once into a PNG: what Etta's wallpaper page
//! shows of each shader before it is applied.
//!
//!   etta-shader-thumb IN.frag OUT.png [--size 848x476] [--base #181a26]
//!                     [--accent #8fa8d8] [--intensity 1.0] [--time 12]
//!
//! Halley's field shaders are GLSL ES fragment shaders with a fixed interface
//! (`v_coords`, `u_time`, `u_resolution`, `u_camera_*`, `u_intensity`,
//! `u_base_color`, `u_accent_color`, `alpha`). Here they are rewritten into
//! Vulkan GLSL —the uniforms gathered in one block—, compiled to SPIR-V by
//! `glslangValidator`, and drawn over the whole frame as Halley draws them at
//! its home view: a 1920 × 1080 camera centred on the origin.

use std::process::{Command, ExitCode};

const VERTEX: &str = r#"
struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn main(@builtin(vertex_index) i: u32) -> Out {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var o: Out;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return o;
}
"#;

const PRELUDE: &str = "#version 450
layout(location = 0) in vec2 v_coords;
layout(location = 0) out vec4 o_color;
layout(set = 0, binding = 0, std140) uniform Field {
    float alpha;
    float u_time;
    float u_intensity;
    float _pad0;
    vec2 u_resolution;
    vec2 u_camera_center;
    vec2 u_camera_size;
    vec2 _pad1;
    vec3 u_base_color;
    float _pad2;
    vec3 u_accent_color;
    float _pad3;
};
";

const KNOWN: [&str; 10] = [
    "tex",
    "alpha",
    "u_resolution",
    "u_camera_center",
    "u_camera_size",
    "u_time",
    "u_intensity",
    "u_base_color",
    "u_accent_color",
    "v_coords",
];

/// The shader as Vulkan GLSL: its own declarations of the interface go, the
/// prelude declares them, and `gl_FragColor` becomes the output.
fn rewrite(src: &str) -> String {
    let mut out = String::from(PRELUDE);
    for line in src.lines() {
        let t = line.trim();
        let declares = (t.starts_with("uniform ") || t.starts_with("varying "))
            && KNOWN
                .iter()
                .any(|k| t.trim_end_matches(';').split_whitespace().last() == Some(*k));
        if t.starts_with("precision ") || t.starts_with("#version") || declares {
            out.push('\n');
            continue;
        }
        out.push_str(&line.replace("gl_FragColor", "o_color"));
        out.push('\n');
    }
    out
}

fn hex(s: &str) -> Option<[f32; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let c = |i: usize| {
        u8::from_str_radix(&s[i..i + 2], 16)
            .ok()
            .map(|v| v as f32 / 255.0)
    };
    Some([c(0)?, c(2)?, c(4)?])
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage: etta-shader-thumb IN.frag OUT.png [--size WxH] [--base #rrggbb] [--accent #rrggbb] [--intensity N] [--time T]"
        );
        return ExitCode::from(2);
    }
    let (input, output) = (&args[1], &args[2]);
    let (mut w, mut h) = (848u32, 476u32);
    let (mut base, mut accent) = ([0.094, 0.102, 0.149], [0.561, 0.659, 0.847]);
    let (mut intensity, mut time) = (1.0f32, 12.0f32);
    let mut k = 3;
    while k + 1 < args.len() {
        let v = &args[k + 1];
        match args[k].as_str() {
            "--size" => {
                if let Some((a, b)) = v.split_once('x') {
                    w = a.parse().unwrap_or(w);
                    h = b.parse().unwrap_or(h)
                }
            }
            "--base" => base = hex(v).unwrap_or(base),
            "--accent" => accent = hex(v).unwrap_or(accent),
            "--intensity" => intensity = v.parse().unwrap_or(intensity),
            "--time" => time = v.parse().unwrap_or(time),
            _ => {}
        }
        k += 2;
    }
    let src = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("shader-thumb · cannot read {input}: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Through glslang, which reports a broken shader with its line.
    let dir = std::env::temp_dir().join(format!("etta-shader-thumb-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let glsl = dir.join("field.frag");
    let spv = dir.join("field.spv");
    if std::fs::write(&glsl, rewrite(&src)).is_err() {
        return ExitCode::FAILURE;
    }
    let compiled = Command::new("glslangValidator")
        .arg("-V")
        .arg("-S")
        .arg("frag")
        .arg("-o")
        .arg(&spv)
        .arg(&glsl)
        .output();
    match compiled {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            eprintln!(
                "shader-thumb · {input} does not compile:\n{}",
                String::from_utf8_lossy(&o.stdout)
            );
            let _ = std::fs::remove_dir_all(&dir);
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("shader-thumb · glslangValidator is needed: {e}");
            return ExitCode::FAILURE;
        }
    }
    let words = std::fs::read(&spv).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let words: Vec<u32> = words
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    match pollster::block_on(paint(&words, w, h, base, accent, intensity, time)) {
        Some(pixels) => {
            let file = match std::fs::File::create(output) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("shader-thumb · cannot write {output}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            match enc
                .write_header()
                .and_then(|mut wr| wr.write_image_data(&pixels))
            {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("shader-thumb · {e}");
                    ExitCode::FAILURE
                }
            }
        }
        None => ExitCode::FAILURE,
    }
}

async fn paint(
    spirv: &[u32],
    w: u32,
    h: u32,
    base: [f32; 3],
    accent: [f32; 3],
    intensity: f32,
    time: f32,
) -> Option<Vec<u8>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .ok()?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .ok()?;
    let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(VERTEX.into()),
    });
    let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::SpirV(spirv.into()),
    });
    // The uniforms, in the block's std140 order.
    let mut u = [0f32; 20];
    u[0] = 1.0;
    u[1] = time;
    u[2] = intensity;
    u[4..6].copy_from_slice(&[1920.0, 1080.0]);
    u[6..8].copy_from_slice(&[0.0, 0.0]);
    u[8..10].copy_from_slice(&[1920.0, 1080.0]);
    u[12..15].copy_from_slice(&base);
    u[16..19].copy_from_slice(&accent);
    let bytes: Vec<u8> = u.iter().flat_map(|f| f.to_le_bytes()).collect();
    let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&ubuf, 0, &bytes);
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: ubuf.as_entire_binding(),
        }],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &vs,
            entry_point: Some("main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs,
            entry_point: Some("main"),
            targets: &[Some(format.into())],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let row = (w * 4).div_ceil(256) * 256;
    let out = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: Default::default(),
            aspect: Default::default(),
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &out,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = out.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().ok()?;
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let start = (y * row) as usize;
        pixels.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    // Opaque: a wallpaper has nothing behind it.
    for p in pixels.chunks_exact_mut(4) {
        p[3] = 255
    }
    Some(pixels)
}
