use super::TextureMut;
use crate::gfx::Texture;
use crate::{
    graphics::QUAD_INDEX_DATA,
    graphics_emu::{
        colored_rect::{ColoredRectUniforms, MAX_COLORED_RECTS},
        dst_texture::DstTexture,
        textured_rect::{TexturedRectUniforms, MAX_TEXTURED_RECTS},
        Graphics,
    },
};
use n64_math::{Color, Vec2, Vec3};
use std::convert::TryInto;
use std::mem;
use wgpu::util::DeviceExt;
use zerocopy::AsBytes;

enum Command {
    ColoredRect {
        upper_left: Vec2,
        lower_right: Vec2,
        color: Color,
    },
    TexturedRect {
        upper_left: Vec2,
        lower_right: Vec2,
        texture: Texture<'static>,
        blend_color: u32,
    },
}

#[derive(Default)]
pub struct CommandBufferCache {
    commands: Vec<Command>,
}

impl CommandBufferCache {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

pub struct CommandBuffer<'a> {
    out_tex: &'a mut TextureMut<'a>,
    clear: bool,
    colored_rect_count: u32,
    textured_rect_count: u32,
    cache: &'a mut CommandBufferCache,
}

impl<'a> CommandBuffer<'a> {
    pub fn new(out_tex: &'a mut TextureMut<'a>, cache: &'a mut CommandBufferCache) -> Self {
        Self {
            out_tex,
            clear: false,
            colored_rect_count: 0,
            textured_rect_count: 0,
            cache,
        }
    }

    pub fn clear(&mut self) -> &mut Self {
        self.clear = true;
        self.cache.commands.clear();
        self
    }

    pub fn add_colored_rect(
        &mut self,
        upper_left: Vec2,
        lower_right: Vec2,
        color: Color,
    ) -> &mut Self {
        self.colored_rect_count += 1;
        self.cache.commands.push(Command::ColoredRect {
            upper_left,
            lower_right,
            color,
        });

        self
    }

    pub fn add_textured_rect(
        &mut self,
        upper_left: Vec2,
        lower_right: Vec2,
        texture: Texture<'static>,
        blend_color: Option<u32>,
    ) -> &mut Self {
        self.textured_rect_count += 1;
        self.cache.commands.push(Command::TexturedRect {
            upper_left,
            lower_right,
            texture,
            blend_color: if let Some(blend_color) = blend_color {
                blend_color
            } else {
                0xff_ff_ff_ff
            },
        });

        self
    }

    pub fn add_mesh_indexed(
        &mut self,
        verts: &[Vec3],
        uvs: &[Vec2],
        colors: &[u32],
        indices: &[[u8; 3]],
        transform: &[[f32; 4]; 4],
        texture: Option<Texture<'static>>,
    ) -> &mut Self {
        self
    }

    pub fn run(self, graphics: &mut Graphics) -> (i32, i32) {
        let dst = DstTexture::new(&graphics.device, self.out_tex.width, self.out_tex.height);
        let window_size = Vec2::new(self.out_tex.width as f32, self.out_tex.height as f32);

        assert!(self.colored_rect_count <= MAX_COLORED_RECTS as u32);
        assert!(self.textured_rect_count <= MAX_TEXTURED_RECTS as u32);

        let command_buf = {
            let mut encoder = graphics
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                let mut colored_rect_uniforms =
                    Vec::with_capacity(self.colored_rect_count as usize);
                let mut textured_rect_uniforms =
                    Vec::with_capacity(self.textured_rect_count as usize);

                for command in &self.cache.commands {
                    match command {
                        Command::ColoredRect {
                            upper_left,
                            lower_right,
                            color,
                        } => {
                            let size = *lower_right - *upper_left;
                            let scale = size / window_size;
                            let offset_x = 2.0 * upper_left.x() / window_size.x() - 1.0 + scale.x();
                            let offset_y =
                                -2.0 * upper_left.y() / window_size.y() + 1.0 - scale.y();

                            colored_rect_uniforms.push(ColoredRectUniforms {
                                color: color.to_rgba(),
                                offset: [offset_x, offset_y],
                                scale: [scale.x(), scale.y()],
                            });
                        }
                        Command::TexturedRect {
                            upper_left,
                            lower_right,
                            texture,
                            blend_color,
                        } => {
                            graphics.textured_rect.upload_texture_data(
                                &graphics.device,
                                &graphics.queue,
                                texture,
                            );

                            let size = *lower_right - *upper_left;
                            let scale = size / window_size;
                            let offset_x = 2.0 * upper_left.x() / window_size.x() - 1.0 + scale.x();
                            let offset_y =
                                -2.0 * upper_left.y() / window_size.y() + 1.0 - scale.y();

                            textured_rect_uniforms.push(TexturedRectUniforms {
                                offset: [offset_x, offset_y],
                                scale: [scale.x(), scale.y()],
                                blend_color: [
                                    ((blend_color >> 24) & 0xff) as f32 / 255.0,
                                    ((blend_color >> 16) & 0xff) as f32 / 255.0,
                                    ((blend_color >> 8) & 0xff) as f32 / 255.0,
                                    ((blend_color >> 0) & 0xff) as f32 / 255.0,
                                ],
                            });
                        }
                    }
                }

                if !colored_rect_uniforms.is_empty() {
                    let temp_buffer =
                        graphics
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: None,
                                contents: colored_rect_uniforms.as_bytes(),
                                usage: wgpu::BufferUsage::COPY_SRC,
                            });

                    encoder.copy_buffer_to_buffer(
                        &temp_buffer,
                        0,
                        &graphics.colored_rect.shader_storage_buffer,
                        0,
                        (colored_rect_uniforms.len() * mem::size_of::<ColoredRectUniforms>())
                            as u64,
                    );
                }

                if !textured_rect_uniforms.is_empty() {
                    let temp_buffer =
                        graphics
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: None,
                                contents: textured_rect_uniforms.as_bytes(),
                                usage: wgpu::BufferUsage::COPY_SRC,
                            });

                    encoder.copy_buffer_to_buffer(
                        &temp_buffer,
                        0,
                        &graphics.textured_rect.shader_storage_buffer,
                        0,
                        (textured_rect_uniforms.len() * mem::size_of::<TexturedRectUniforms>())
                            as u64,
                    );
                }
            }

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &dst.tex_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if self.clear {
                                wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 1.0,
                                })
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: true,
                        },
                    }],
                    depth_stencil_attachment: None,
                });

                render_pass.set_index_buffer(graphics.quad_index_buf.slice(..));
                render_pass.set_vertex_buffer(0, graphics.quad_vertex_buf.slice(..));

                {
                    let mut colored_rect_index = 0;
                    let mut textured_rect_index = 0;

                    for command in &self.cache.commands {
                        match command {
                            Command::ColoredRect { .. } => {
                                render_pass.set_pipeline(&graphics.colored_rect.pipeline);
                                render_pass.set_bind_group(
                                    0,
                                    &graphics.colored_rect.bind_group,
                                    &[],
                                );
                                render_pass.draw_indexed(
                                    0..(QUAD_INDEX_DATA.len() as u32),
                                    0,
                                    colored_rect_index..(colored_rect_index + 1),
                                );
                                colored_rect_index += 1;
                            }
                            Command::TexturedRect { texture, .. } => {
                                render_pass.set_pipeline(&graphics.textured_rect.pipeline);
                                render_pass.set_bind_group(
                                    0,
                                    &graphics
                                        .textured_rect
                                        .texture_cache
                                        .get(&(texture.data as *const _))
                                        .unwrap()
                                        .bind_group,
                                    &[],
                                );
                                render_pass.draw_indexed(
                                    0..(QUAD_INDEX_DATA.len() as u32),
                                    0,
                                    textured_rect_index..(textured_rect_index + 1),
                                );
                                textured_rect_index += 1;
                            }
                        }
                    }
                }
            }

            encoder.copy_texture_to_buffer(
                wgpu::TextureCopyView {
                    texture: &dst.tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                },
                wgpu::BufferCopyView {
                    buffer: &dst.buffer,
                    layout: wgpu::TextureDataLayout {
                        offset: 0,
                        bytes_per_row: 4 * self.out_tex.width as u32,
                        rows_per_image: self.out_tex.height as u32,
                    },
                },
                dst.tex_extent,
            );

            encoder.finish()
        };

        graphics.queue.submit(Some(command_buf));

        {
            futures_executor::block_on(async {
                dst.buffer
                    .slice(0..((4 * self.out_tex.width * self.out_tex.height) as u64))
                    .map_async(wgpu::MapMode::Read)
                    .await
            })
            .unwrap();

            let mapped_colored_rect_dst_buffer = dst
                .buffer
                .slice(0..((4 * self.out_tex.width * self.out_tex.height) as u64))
                .get_mapped_range();

            for (fb_color, mapped_color) in self
                .out_tex
                .data
                .iter_mut()
                .zip(mapped_colored_rect_dst_buffer.chunks(4))
            {
                *fb_color = Color::from_bytes(mapped_color.try_into().unwrap());
            }
        }

        (
            self.colored_rect_count as i32,
            self.textured_rect_count as i32,
        )
    }
}
