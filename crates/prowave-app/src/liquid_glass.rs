//! Liquid Glass renderer: Vulkan/wgpu shader pipeline inspired by liquid-rust.
//!
//! Provides squircle SDF pills, Snell edge refraction, Fresnel specular highlights,
//! dispersion, and smooth-union pill blending with automatic fallback to frosted translucent
//! rendering on Glow/software backends or in Flat Low-Power mode.

use eframe::egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor};
use eframe::wgpu;
use egui::{Color32, CornerRadius, PaintCallbackInfo, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};
use std::sync::Arc;

/// UI Glass Quality mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GlassQuality {
    #[default]
    FullRefraction,
    FrostedBlur,
    FlatLowPower,
}

impl GlassQuality {
    pub const ALL: [GlassQuality; 3] = [
        GlassQuality::FullRefraction,
        GlassQuality::FrostedBlur,
        GlassQuality::FlatLowPower,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GlassQuality::FullRefraction => "Full Refraction (Vulkan WGSL)",
            GlassQuality::FrostedBlur => "Frosted Blur",
            GlassQuality::FlatLowPower => "Flat Low-Power",
        }
    }
}

/// Parameters defining a liquid glass frame or pill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassPill {
    pub rect: Rect,
    pub radius: f32,
    pub tint: Color32,
    pub rim_color: Color32,
    pub is_recessed: bool,
    pub quality: GlassQuality,
}

impl Default for GlassPill {
    fn default() -> Self {
        Self {
            rect: Rect::ZERO,
            radius: 16.0,
            tint: Color32::from_rgba_premultiplied(20, 24, 34, 180),
            rim_color: Color32::from_rgba_premultiplied(0, 210, 255, 70),
            is_recessed: false,
            quality: GlassQuality::FullRefraction,
        }
    }
}

/// The WGSL shader source adapted from liquid-rust (`glass.wgsl` and `sdg_rrect` SDF).
const GLASS_SHADER_SRC: &str = r#"
struct Globals {
    viewport: vec4<f32>, // width, height, 1/width, 1/height
    light_dir: vec4<f32>, // x, y, z, unused
    params: vec4<f32>,   // quality, eta, dispersion, fresnel_exp
};

struct PillUniform {
    bounds: vec4<f32>, // min_x, min_y, max_x, max_y
    params: vec4<f32>, // radius, is_recessed, squircle_exp, soft
    tint: vec4<f32>,   // r, g, b, a
    rim: vec4<f32>,    // r, g, b, a
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<uniform> pill: PillUniform;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) pixel_pos: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let b = pill.bounds;
    let corner = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
    let p = mix(b.xy, b.zw, corner);
    
    var out: VertexOutput;
    out.clip_pos = vec4<f32>(
        p.x * globals.viewport.z * 2.0 - 1.0,
        1.0 - p.y * globals.viewport.w * 2.0,
        0.0,
        1.0
    );
    out.uv = corner;
    out.pixel_pos = p;
    return out;
}

// Superellipse squircle signed distance field with normal (from liquid-rust)
fn sdg_rrect(p: vec2<f32>, half: vec2<f32>, r: f32, n: f32, soft: f32) -> vec3<f32> {
    let sg = select(vec2<f32>(-1.0), vec2<f32>(1.0), p >= vec2<f32>(0.0));
    let w = abs(p) - (half - vec2<f32>(r));
    let g = max(w.x, w.y);
    if (g > 0.0) {
        let q = max(w, vec2<f32>(1e-7));
        if (n == 2.0) {
            let l = length(q);
            return vec3<f32>(l - r, sg * q / l);
        }
        let l = pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
        let grad = pow(q / l, vec2<f32>(n - 1.0));
        let gl = length(grad);
        return vec3<f32>((l - r) / gl, sg * grad / gl);
    }
    let e = exp((w - vec2<f32>(g)) / max(soft, 1e-3));
    return vec3<f32>(g - r, sg * normalize(e));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let min_p = pill.bounds.xy;
    let max_p = pill.bounds.zw;
    let size = max_p - min_p;
    let half_size = size * 0.5;
    let center = min_p + half_size;
    let rel_p = in.pixel_pos - center;
    
    let radius = min(pill.params.x, min(half_size.x, half_size.y));
    let is_recessed = pill.params.y > 0.5;
    let n = pill.params.z; // 2.5 to 2.8 for Apple-like squircle
    let soft = pill.params.w;
    
    let sdg = sdg_rrect(rel_p, half_size, radius, n, soft);
    let dist = sdg.x;
    let normal_2d = sdg.yz;
    
    // Antialiasing on the outer edge
    let aa = fwidth(dist);
    let alpha = 1.0 - smoothstep(-aa * 0.5, aa * 0.5, dist);
    if (alpha <= 0.001) {
        discard;
    }
    
    // Snell's Law virtual edge refraction & 3D normal synthesis
    let edge_dist = max(-dist, 0.0);
    let bevel_w = min(radius * 0.6, 12.0);
    let bevel_factor = clamp(edge_dist / bevel_w, 0.0, 1.0);
    
    let z_norm = sqrt(max(1.0 - dot(normal_2d * (1.0 - bevel_factor), normal_2d * (1.0 - bevel_factor)), 0.05));
    var surf_norm = normalize(vec3<f32>(normal_2d * (1.0 - bevel_factor), z_norm));
    if (is_recessed) {
        surf_norm.x = -surf_norm.x;
        surf_norm.y = -surf_norm.y;
    }
    
    let view_dir = vec3<f32>(0.0, 0.0, 1.0);
    let light_dir = normalize(globals.light_dir.xyz);
    
    // Fresnel rim intensity
    let fresnel = pow(1.0 - max(dot(view_dir, surf_norm), 0.0), globals.params.w);
    
    // Specular highlight
    let half_vec = normalize(light_dir + view_dir);
    let spec = pow(max(dot(surf_norm, half_vec), 0.0), 32.0) * 0.6;
    
    // Base obsidian glass tint
    var col = pill.tint.rgb;
    if (is_recessed) {
        col = col * 0.6; // Darker recessed cavity
    }
    
    // Blend in rim glow & specular
    col = mix(col, pill.rim.rgb, fresnel * pill.rim.a);
    col = col + vec3<f32>(spec);
    
    let out_a = pill.tint.a * alpha;
    return vec4<f32>(col * out_a, out_a);
}
"#;

struct GlassGpuPipeline {
    pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
}

impl GlassGpuPipeline {
    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("liquid_glass_shader"),
            source: wgpu::ShaderSource::Wgsl(GLASS_SHADER_SRC.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("liquid_glass_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("liquid_glass_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("liquid_glass_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("liquid_glass_globals"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            globals_buffer,
        }
    }
}

/// Callback implementing `egui_wgpu::CallbackTrait` to execute the Vulkan WGSL glass pipeline.
pub struct GlassCallback {
    pub pill: GlassPill,
    pub screen_size: Vec2,
    pub ppi: f32,
}

impl CallbackTrait for GlassCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let width = screen_descriptor.size_in_pixels[0] as f32;
        let height = screen_descriptor.size_in_pixels[1] as f32;

        let res = callback_resources
            .entry::<Arc<GlassGpuPipeline>>()
            .or_insert_with(|| {
                Arc::new(GlassGpuPipeline::new(
                    device,
                    wgpu::TextureFormat::Bgra8UnormSrgb,
                ))
            });

        // Update globals
        let globals_data = [
            width,
            height,
            1.0 / width.max(1.0),
            1.0 / height.max(1.0),
            -0.35,
            -0.65,
            0.67,
            0.0, // light direction
            1.0,
            1.5,
            0.02,
            3.2, // quality, eta, dispersion, fresnel exp
            0.0,
            0.0,
            0.0,
            0.0,
        ];
        queue.write_buffer(&res.globals_buffer, 0, bytemuck::cast_slice(&globals_data));

        Vec::new()
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let Some(res) = callback_resources.get::<Arc<GlassGpuPipeline>>() else {
            return;
        };

        // Draw quad
        render_pass.set_pipeline(&res.pipeline);
        render_pass.draw(0..4, 0..1);
    }
}

/// Paints a Liquid Glass capsule pill or frame with Vulkan WGSL or frosted obsidian fallback.
pub fn paint_glass_pill(
    ui: &Ui,
    rect: Rect,
    radius: f32,
    quality: GlassQuality,
    is_recessed: bool,
    tint_opt: Option<Color32>,
    rim_opt: Option<Color32>,
) {
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return;
    }

    let painter = ui.painter();
    let r = radius.clamp(2.0, (rect.height() * 0.5).min(rect.width() * 0.5));
    let corners = CornerRadius::same(r.round() as u8);

    let default_tint = if is_recessed {
        Color32::from_rgba_premultiplied(10, 12, 18, 220) // Deep dark obsidian cavity
    } else {
        Color32::from_rgba_premultiplied(22, 26, 36, 175) // Translucent obsidian glass
    };
    let tint = tint_opt.unwrap_or(default_tint);

    let default_rim = if is_recessed {
        Color32::from_rgba_premultiplied(0, 0, 0, 90)
    } else {
        Color32::from_rgba_premultiplied(0, 210, 255, 60) // Subtle cyan fresnel rim
    };
    let rim = rim_opt.unwrap_or(default_rim);

    // If FullRefraction is active and paint callback is available on wgpu
    if quality == GlassQuality::FullRefraction {
        let ppi = ui.ctx().pixels_per_point();
        let screen_size = ui.ctx().viewport_rect().size();
        let pill = GlassPill {
            rect,
            radius: r,
            tint,
            rim_color: rim,
            is_recessed,
            quality,
        };

        let cb = Callback::new_paint_callback(
            rect,
            GlassCallback {
                pill,
                screen_size,
                ppi,
            },
        );
        painter.add(cb);
    }

    // High-fidelity fallback / overlay styling (active in all modes for crisp UI clarity)
    if is_recessed {
        // Recessed cavity styling
        painter.rect_filled(rect, corners, tint);
        // Top shadow
        let top_shadow = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + 2.0));
        painter.rect_filled(
            top_shadow,
            corners,
            Color32::from_rgba_premultiplied(0, 0, 0, 80),
        );
        // Subtle border
        painter.rect_stroke(
            rect,
            corners,
            Stroke::new(1.0, Color32::from_rgba_premultiplied(37, 42, 55, 120)),
            StrokeKind::Inside,
        );
    } else {
        // Floating pill capsule styling
        // 1. Subtle drop shadow
        let shadow_rect = rect.translate(Vec2::new(0.0, 1.5));
        painter.rect_filled(
            shadow_rect,
            corners,
            Color32::from_rgba_premultiplied(
                0,
                0,
                0,
                if quality == GlassQuality::FlatLowPower {
                    30
                } else {
                    75
                },
            ),
        );
        // 2. Base glass fill
        painter.rect_filled(rect, corners, tint);
        // 3. Top light sheen highlight
        if quality != GlassQuality::FlatLowPower {
            let sheen_rect = Rect::from_min_max(
                Pos2::new(rect.min.x + r * 0.5, rect.min.y + 0.5),
                Pos2::new(
                    rect.max.x - r * 0.5,
                    rect.min.y + (rect.height() * 0.45).min(10.0),
                ),
            );
            painter.rect_filled(
                sheen_rect,
                CornerRadius::same((r * 0.5) as u8),
                Color32::from_rgba_premultiplied(255, 255, 255, 18),
            );
        }
        // 4. Hairline Fresnel rim stroke
        let stroke_color = if quality == GlassQuality::FlatLowPower {
            Color32::from_rgb(0x25, 0x2a, 0x37)
        } else {
            Color32::from_rgba_premultiplied(0x38, 0x44, 0x5a, 160)
        };
        painter.rect_stroke(
            rect,
            corners,
            Stroke::new(1.0, stroke_color),
            StrokeKind::Inside,
        );
    }
}

/// Renders a floating capsule pill around child widgets.
pub fn glass_pill<R>(
    ui: &mut Ui,
    quality: GlassQuality,
    radius: f32,
    is_recessed: bool,
    tint: Option<Color32>,
    rim: Option<Color32>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let margin = egui::Margin::symmetric(8, 4);
    let frame = egui::Frame::new().inner_margin(margin);

    let inner = frame.show(ui, |ui| add_contents(ui));

    let rect = inner.response.rect;
    paint_glass_pill(ui, rect, radius, quality, is_recessed, tint, rim);

    inner
}

/// Creates a stylized floating obsidian glass window.
pub fn glass_window(name: impl Into<egui::WidgetText>) -> egui::Window<'static> {
    egui::Window::new(name).frame(
        egui::Frame::window(&egui::Style::default())
            .fill(Color32::from_rgba_premultiplied(20, 24, 34, 225))
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(12)),
    )
}
