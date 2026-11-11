//! The book on the GPU (`warp.wgsl`): its pages' curve, and the sheet turning over them.
//!
//! The book view puts an effect slot (`slot`) over the spread. Rune copies everything drawn
//! before it into a backdrop, and the shader redraws that through the pages' surface, so the
//! writing bends with the paper. While a sheet turns face up on the right, the view first
//! draws it lying flat, writing and all, under a second slot (`capture`) whose backdrop the
//! compositor copies into a texture it keeps, then draws the page beneath over it again; the
//! shader draws the capture squeezed to the width the sheet shows at, so its writing turns
//! with it (Rune scales elements only evenly). A leaf back up on the left is drawn from the
//! left page in the backdrop itself, writing and all. Lying flat, each sheet is exactly the
//! page it lifts off or lands on, so a turn neither starts nor ends with a jump.
//!
//! `Warp` is the app's compositor: frames without the book draw as Rune would, and backdrops
//! are only copied while it's up. Headless renders go through it too.

use rune::compose::{Compositor, DeviceKey, Frame};
use rune::render::{EffectPhase, EffectSite, KIND_EFFECT};
use rune::{El, effect};

use super::{BOOK_H, BOOK_W, PAGE_H, PAGE_INSET_X, PAGE_INSET_Y, SPINE};
use crate::store::Cantrip;

/// The book's effect slot tags.
const CURVE: u32 = 0xB00C;
const CAPTURE: u32 = 0xB00D;

/// How far the light rakes across the pages' slopes.
const RAKE: f32 = 0.9;
/// How far past the book the slots reach, for pages bulging over its edges.
const BLEED: f32 = 12.0;

fn over_book(mark: u32, key: &'static str) -> El {
    effect(mark)
        .key(key)
        .a11y_hidden()
        .inert()
        .effect_backdrop(true)
        .pin(Some(-BLEED), Some(-BLEED), None, None)
        .size(BOOK_W + 2.0 * BLEED, BOOK_H + 2.0 * BLEED)
}

/// The slot over a `BOOK_W` × `BOOK_H` spread: its pages curve by `strength` (0 flat, 1 open).
pub fn slot(strength: f32) -> El {
    over_book(CURVE, "page-curve").effect_data([strength.clamp(0.0, 1.0), 0.0, 0.0, 0.0])
}

/// The slot that captures the turning sheet drawn flat before it. It draws nothing.
pub fn capture() -> El {
    over_book(CAPTURE, "page-capture")
}

/// The turning sheet as last captured.
struct Captured {
    size: [u32; 2],
    format: wgpu::TextureFormat,
    view: wgpu::TextureView,
}

/// Pipelines for one target format.
struct Pipes {
    format: wgpu::TextureFormat,
    curve: wgpu::RenderPipeline,
    blit: wgpu::RenderPipeline,
}

/// Everything made on one device.
struct Gpu {
    device: DeviceKey,
    pipes: Vec<Pipes>,
    curve_layout: wgpu::BindGroupLayout,
    blit_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// Bound for the sheet when none is turning.
    blank: wgpu::TextureView,
    captured: Option<Captured>,
}

#[derive(Default)]
pub struct Warp {
    gpu: Option<Gpu>,
}

impl Compositor<Cantrip> for Warp {
    fn render(&mut self, f: &mut Frame<'_, Cantrip>) {
        let marks = &f.rt.marks;
        let book =
            f.rt.instances
                .iter()
                .enumerate()
                .any(|(n, i)| i.params[1] == KIND_EFFECT && marks.get(n) == Some(&CURVE));
        if !book {
            return f.draw_ui();
        }
        let (device, queue, size, key) = (f.device, f.queue, f.size, f.device_key());
        if self.gpu.as_ref().is_none_or(|g| g.device != key) {
            self.gpu = Some(Gpu::new(device, key));
        }
        let Some(gpu) = self.gpu.as_mut() else { return };
        let (recto, verso) = super::sheets(&f.rt.state);
        let mut captured = false;
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        f.ui.draw_with_backdrop(f.encoder, f.target, clear, &mut |phase, site| {
            let Some(backdrop) = site.backdrop else {
                return;
            };
            match (site.mark, phase) {
                (CAPTURE, EffectPhase::Backdrop(encoder)) => {
                    gpu.capture(device, encoder, site, backdrop);
                    captured = true;
                }
                (CURVE, EffectPhase::Draw(pass)) => {
                    let recto = recto.filter(|_| captured);
                    gpu.curve(device, queue, pass, site, backdrop, recto, verso);
                }
                _ => {}
            }
        });
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, key: DeviceKey) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let curve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("page curve"),
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
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(3),
            ],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("page capture"),
            entries: &[texture(0)],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("page curve"),
            size: 40 * 4,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("page curve"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blank = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("no sheet"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Gpu {
            device: key,
            pipes: Vec::new(),
            curve_layout,
            blit_layout,
            uniforms,
            sampler,
            blank,
            captured: None,
        }
    }

    fn pipes(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) -> &Pipes {
        if let Some(at) = self.pipes.iter().position(|p| p.format == format) {
            return &self.pipes[at];
        }
        let pipeline = |label, source: &str, layout: &wgpu::BindGroupLayout, strip: bool| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        // Both redraw what's there: replace, don't blend.
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: if strip {
                        wgpu::PrimitiveTopology::TriangleStrip
                    } else {
                        wgpu::PrimitiveTopology::TriangleList
                    },
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let curve = pipeline(
            "page curve",
            include_str!("warp.wgsl"),
            &self.curve_layout,
            true,
        );
        let blit = pipeline(
            "page capture",
            include_str!("blit.wgsl"),
            &self.blit_layout,
            false,
        );
        self.pipes.push(Pipes {
            format,
            curve,
            blit,
        });
        &self.pipes[self.pipes.len() - 1]
    }

    /// Copies the backdrop at the capture slot (the turning sheet, lying flat) into a texture
    /// of its own, while the pass is closed.
    fn capture(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        site: &EffectSite<'_>,
        backdrop: &wgpu::TextureView,
    ) {
        let fits = self
            .captured
            .as_ref()
            .is_some_and(|c| c.size == site.size && c.format == site.format);
        if !fits {
            let view = device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("turning sheet"),
                    size: wgpu::Extent3d {
                        width: site.size[0].max(1),
                        height: site.size[1].max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: site.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default());
            self.captured = Some(Captured {
                size: site.size,
                format: site.format,
                view,
            });
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("page capture"),
            layout: &self.blit_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(backdrop),
            }],
        });
        let blit = self.pipes(device, site.format).blit.clone();
        let Some(captured) = &self.captured else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("page capture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &captured.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&blit);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Redraws the backdrop at the curve slot through the pages' surface, and the sheets in
    /// the air over it: the captured `recto`, and the `verso` from the backdrop.
    #[allow(clippy::too_many_arguments)]
    fn curve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        site: &EffectSite<'_>,
        backdrop: &wgpu::TextureView,
        recto: Option<super::Sheet>,
        verso: Option<super::Sheet>,
    ) {
        let r = site.instance.rect;
        let sheet = |s: Option<super::Sheet>| match s {
            Some(s) => [
                s.flat,
                s.now,
                [1.0, s.stand, s.fall, if s.cover { 1.0 } else { 0.0 }],
            ],
            None => [[0.0; 4]; 3],
        };
        let mut values: Vec<f32> = vec![
            r[0] - site.origin[0],
            r[1] - site.origin[1],
            r[2],
            r[3],
            site.scale,
            site.data[0],
            site.size[0] as f32,
            site.size[1] as f32,
            BOOK_W,
            BOOK_H,
            SPINE,
            PAGE_INSET_X,
            PAGE_INSET_Y,
            PAGE_INSET_Y + PAGE_H,
            RAKE,
            BLEED,
        ];
        values.extend(sheet(recto).into_iter().flatten());
        values.extend(sheet(verso).into_iter().flatten());
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
        queue.write_buffer(&self.uniforms, 0, &bytes);
        let sheet = match (&self.captured, recto) {
            (Some(c), Some(_)) if c.format == site.format => &c.view,
            _ => &self.blank,
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("page curve"),
            layout: &self.curve_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(sheet),
                },
            ],
        });
        let curve = self.pipes(device, site.format).curve.clone();
        pass.set_pipeline(&curve);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..4, 0..1);
    }
}

#[cfg(test)]
mod tests {
    /// A shader that doesn't compile takes the whole app down when the book opens.
    #[test]
    fn the_shaders_compile() {
        for (name, source) in [
            ("warp.wgsl", include_str!("warp.wgsl")),
            ("blit.wgsl", include_str!("blit.wgsl")),
        ] {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
    }
}
