//! The pages' curve, on the GPU (`warp.wgsl`). The book view puts an effect slot (`slot`) over
//! the spread; Rune copies everything drawn before it into a backdrop, and the shader redraws
//! that through the pages' surface, so the writing bends with the paper.
//!
//! `Warp` is the app's compositor: frames without the slot draw as Rune would, and the slot's
//! backdrop is only copied while the book is up. Headless renders go through it too.

use rune::compose::{Compositor, Frame};
use rune::render::{EffectPhase, EffectSite, KIND_EFFECT};
use rune::{El, effect};

use super::{BOOK_H, BOOK_W, PAGE_H, PAGE_INSET_X, PAGE_INSET_Y, SPINE};
use crate::store::Cantrip;

/// The book's effect slot tag.
const MARK: u32 = 0xB00C;

/// How far the light rakes across the pages' slopes.
const RAKE: f32 = 0.9;
/// How far past the book the slot reaches, for pages bulging over its edges.
const BLEED: f32 = 12.0;

/// The slot over a `BOOK_W` × `BOOK_H` spread: its pages curve by `strength` (0 flat, 1 open).
pub fn slot(strength: f32) -> El {
    effect(MARK)
        .key("page-curve")
        .a11y_hidden()
        .inert()
        .effect_backdrop(true)
        .effect_data([strength.clamp(0.0, 1.0), 0.0, 0.0, 0.0])
        .pin(Some(-BLEED), Some(-BLEED), None, None)
        .size(BOOK_W + 2.0 * BLEED, BOOK_H + 2.0 * BLEED)
}

struct Pipeline {
    device: rune::compose::DeviceKey,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

#[derive(Default)]
pub struct Warp {
    pipelines: Vec<Pipeline>,
}

impl Compositor<Cantrip> for Warp {
    fn render(&mut self, f: &mut Frame<'_, Cantrip>) {
        let marks = &f.rt.marks;
        let curved =
            f.rt.instances
                .iter()
                .enumerate()
                .any(|(n, i)| i.params[1] == KIND_EFFECT && marks.get(n) == Some(&MARK));
        if !curved {
            return f.draw_ui();
        }
        let (device, queue, size, key) = (f.device, f.queue, f.size, f.device_key());
        self.pipelines.retain(|p| p.device == key);
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        let pipelines = &mut self.pipelines;
        f.ui.draw_with_backdrop(f.encoder, f.target, clear, &mut |phase, site| {
            if site.mark != MARK {
                return;
            }
            let (EffectPhase::Draw(pass), Some(backdrop)) = (phase, site.backdrop) else {
                return;
            };
            let at = match pipelines.iter().position(|p| p.format == site.format) {
                Some(at) => at,
                None => {
                    pipelines.push(Pipeline::new(device, key, site.format));
                    pipelines.len() - 1
                }
            };
            pipelines[at].draw(device, queue, pass, site, backdrop);
        });
    }
}

impl Pipeline {
    fn new(
        device: &wgpu::Device,
        key: rune::compose::DeviceKey,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("page curve"),
            source: wgpu::ShaderSource::Wgsl(include_str!("warp.wgsl").into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("page curve"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("page curve"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("page curve"),
            layout: Some(&pipeline_layout),
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
                    // The slot redraws what's under it: replace, don't blend.
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("page curve"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("page curve"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Pipeline {
            device: key,
            format,
            pipeline,
            layout,
            uniforms,
            sampler,
        }
    }

    fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        site: &EffectSite<'_>,
        backdrop: &wgpu::TextureView,
    ) {
        let r = site.instance.rect;
        let values: [f32; 16] = [
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
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
        queue.write_buffer(&self.uniforms, 0, &bytes);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("page curve"),
            layout: &self.layout,
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
            ],
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..4, 0..1);
    }
}
