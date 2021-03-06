use super::{Texture, TextureMut};
use crate::graphics::Graphics;
use n64_math::{Color, Vec2, Vec3};
use n64_sys::rdp;
use rdp_command_builder::*;

mod rdp_command_builder;

fn float_to_int_frac(val: f32) -> (u16, u16) {
    let integer_part = libm::floorf(val);
    let fractal_part = val - integer_part;
    (integer_part as u16, libm::floorf(fractal_part * ((1 << 16) as f32)) as u16)
}

// Dx/Dy of edge from p0 to p1.
fn edge_slope(p0: Vec3, p1: Vec3) -> (u16, u16) {
    // TODO: ZERO DIVISION CHECK
    float_to_int_frac((p1.0 - p0.0) / (p1.1 - p0.1))
}

// X coordinate of the intersection of the edge from p0 to p1 and the sub-scanline at (or higher than) p0.y
fn slope_y_next_subpixel_intersection(p0: Vec3, p1: Vec3) -> (u16, u16){

    let y = libm::ceilf(p0.1*4.0) / 4.0;

    let x = p0.0 + (y - p0.1)*(p1.0 - p0.0) / (p1.1 - p0.1);
    float_to_int_frac(x) 
}

fn slope_y_prev_scanline_intersection(p0: Vec3, p1: Vec3) -> (u16, u16){
    let y = libm::floorf(p0.1);
    // kx + m = y
    // k = (p1y-p0y)/(p1x-p0x)
    // m = p0.y
    // x = (y - p0.y)*(p1x-p0x) / (p1y-p0y)
    // TODO ZERO DIVISION
    let x = p0.0 + (y - p0.1)*(p1.0 - p0.0) / (p1.1 - p0.1);

    float_to_int_frac(x) 
}

fn int_frac_greater(a_integer : u16, a_fraction : u16, b_integer : u16, b_fraction : u16) -> bool
{
    if a_integer == b_integer
    {
        a_fraction > b_fraction
    }
    else
    {
        a_integer > b_integer
    }
}

// Sort so taht v0.1 <= v1.1 <= v2.1
fn sorted_triangle(v0 : Vec3, v1 : Vec3, v2 : Vec3) -> (Vec3, Vec3, Vec3) {
    if v0.1 > v1.1 {
        sorted_triangle(v1, v0, v2)
    }
    else if v0.1 > v2.1 {
        sorted_triangle(v1, v2, v0)
    }
    else if v1.1 > v2.1 {
        sorted_triangle(v0, v2, v1)
    }
    else {
        (v0, v1, v2)
    }
}

pub struct CommandBufferCache {
    rdp: RdpCommandBuilder,
}

impl CommandBufferCache {
    pub fn new() -> Self {
        Self {
            rdp: RdpCommandBuilder::new(),
        }
    }
}

pub struct CommandBuffer<'a> {
    out_tex: &'a mut TextureMut<'a>,
    colored_rect_count: u32,
    textured_rect_count: u32,
    cache: &'a mut CommandBufferCache,
}

impl<'a> CommandBuffer<'a> {
    pub fn new(out_tex: &'a mut TextureMut<'a>, cache: &'a mut CommandBufferCache) -> Self {
        cache.rdp.clear();

        cache
            .rdp
            .set_color_image(
                FORMAT_RGBA,
                SIZE_OF_PIXEL_16B,
                out_tex.width as u16,
                out_tex.data.as_mut_ptr() as *mut u16,
            )
            .set_scissor(
                Vec2::zero(),
                Vec2::new((out_tex.width - 1) as f32, (out_tex.height - 1) as f32),
            )
            .set_combine_mode(&[0, 0, 0, 0, 6, 1, 0, 15, 1, 0, 0, 0, 0, 7, 7, 7]);

        CommandBuffer {
            out_tex,
            colored_rect_count: 0,
            textured_rect_count: 0,
            cache,
        }
    }

    pub fn clear(&mut self) -> &mut Self {
        self.cache
            .rdp
            .set_other_modes(
                OTHER_MODE_CYCLE_TYPE_FILL
                    | OTHER_MODE_CYCLE_TYPE_COPY
                    | OTHER_MODE_CYCLE_TYPE_2_CYCLE
                    | OTHER_MODE_RGB_DITHER_SEL_NO_DITHER
                    | OTHER_MODE_ALPHA_DITHER_SEL_NO_DITHER
                    | OTHER_MODE_FORCE_BLEND,
            )
            .set_fill_color(Color::new(0b00000_00000_00000_1))
            .fill_rectangle(
                Vec2::new(0.0, 0.0),
                Vec2::new(
                    (self.out_tex.width - 1) as f32,
                    (self.out_tex.height - 1) as f32,
                ),
            );

        self
    }

    pub fn add_colored_rect(
        &mut self,
        upper_left: Vec2,
        lower_right: Vec2,
        color: Color,
    ) -> &mut Self {
        self.colored_rect_count += 1;
        self.cache
            .rdp
            .sync_pipe()
            .set_other_modes(
                OTHER_MODE_CYCLE_TYPE_FILL
                    | OTHER_MODE_CYCLE_TYPE_COPY
                    | OTHER_MODE_CYCLE_TYPE_1_CYCLE
                    | OTHER_MODE_RGB_DITHER_SEL_NO_DITHER
                    | OTHER_MODE_ALPHA_DITHER_SEL_NO_DITHER
                    | OTHER_MODE_FORCE_BLEND,
            )
            .set_combine_mode(&[0, 0, 0, 0, 6, 1, 0, 15, 1, 0, 0, 0, 0, 7, 7, 7])
            .set_fill_color(color)
            .fill_rectangle(upper_left, lower_right - Vec2::new(1.0, 1.0));

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
        self.cache.rdp.sync_tile().set_other_modes(
            OTHER_MODE_SAMPLE_TYPE
                | OTHER_MODE_BI_LERP_0
                | OTHER_MODE_ALPHA_DITHER_SEL_NO_DITHER
                | OTHER_MODE_B_M2A_0_1
                | if let Some(_) = blend_color {
                    OTHER_MODE_B_M1A_0_2
                } else {
                    0
                }
                | OTHER_MODE_FORCE_BLEND
                | OTHER_MODE_IMAGE_READ_EN,
        );

        if let Some(blend_color) = blend_color {
            self.cache.rdp.set_blend_color(blend_color);
        }

        self.cache
            .rdp
            .set_texture_image(
                FORMAT_RGBA,
                SIZE_OF_PIXEL_16B,
                texture.width as u16,
                texture.data.as_ptr() as *const u16,
            )
            .set_tile(
                FORMAT_RGBA,
                SIZE_OF_PIXEL_16B,
                texture.width as u16,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            )
            .load_tile(
                Vec2::new((texture.width) as f32, (texture.height) as f32),
                Vec2::new(0.0, 0.0),
                0,
            )
            .texture_rectangle(
                upper_left,
                lower_right,
                0,
                Vec2::new(0.0, 0.0),
                Vec2::new(32.0, 32.0),
            );
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

        self.cache
            .rdp
            .set_fill_color(Color::new(0b10000_00011_00011_1));
            
        // Set triangle mode fill
         self.cache
            .rdp
            .set_other_modes(3u64 <<52);
        for triangle in indices {
            // TODO: Transform before sort
            let mut v0 = verts[triangle[0] as usize];
            let mut v1 = verts[triangle[1] as usize];
            let mut v2 = verts[triangle[2] as usize];

            let scale = 15.0;

            v0.0 = 4.0 * libm::fmaxf(libm::fminf( scale * (1.0 + v0.0), 128.0), 0.0);
            v1.0 = 4.0 * libm::fmaxf(libm::fminf( scale * (1.0 + v1.0), 128.0), 0.0);
            v2.0 = 4.0 * libm::fmaxf(libm::fminf( scale * (1.0 + v2.0), 128.0), 0.0);
            v0.1 = 4.0 * libm::fmaxf(libm::fminf( scale * (1.0 + v0.1), 128.0), 0.0);
            v1.1 = 4.0 * libm::fmaxf(libm::fminf( scale * (1.0 + v1.1), 128.0), 0.0);
            v2.1 = 4.0 * libm::fmaxf(libm::fminf( scale * (1.0 + v2.1), 128.0), 0.0);

            let (vh, vm, vl) = sorted_triangle(v0, v1, v2);
            
            // panic!("V012\n{}\n{}\n{}\nVHML\n{}\n{}\n{}", v0, v1, v2, vh, vm, vl);

            //TODO: Actual intersections (low with subpixel, mid & high with previous scanline)
            //
            let (l_int, l_frac) = slope_y_next_subpixel_intersection(vl, vh);
            let (m_int, m_frac) = slope_y_prev_scanline_intersection(vh, vm);
            let (h_int, h_frac) = slope_y_prev_scanline_intersection(vh, vl);

            // panic!("{}\n{}\n{}\n{}\n{}\n{}", l_int, l_frac, m_int, m_frac, h_int, h_frac);

            // TODO: Special care if on same y coord
            let (l_slope_int, l_slope_frac) = edge_slope(vl, vm);
            let (m_slope_int, m_slope_frac) = edge_slope(vm, vh);
            let (h_slope_int, h_slope_frac) = edge_slope(vl, vh);

            // panic!("{}\n{}\n{}\n{}\n{}\n{}", l_slope_int, l_slope_frac, m_slope_int, m_slope_frac, h_slope_int, h_slope_frac);

            //panic!("{}.{}>{}.{}\n{}", m_int, m_frac, h_int, h_frac, int_frac_greater(m_int, m_frac, h_int, h_frac));

            self.cache.rdp.edge_coefficients(
                false,
                false,
                false,
                int_frac_greater(m_int, m_frac, h_int, h_frac),
                0,
                0,
                vl.1,
                vm.1,
                vh.1,
                l_int,
                l_frac,
                m_int,
                m_frac,
                h_int,
                h_frac,
                l_slope_int,
                l_slope_frac,
                m_slope_int,
                m_slope_frac,
                h_slope_int,
                h_slope_frac,
            );
            
            /*panic!(
                "Vl {} {}\nVm {} {}\nVh {} {}\nLY,X {} {} {}\nMY,X {} {} {}\nHY,X {} {} {}",
            vl.1, vl.0,
            vm.1, vm.0,
            vh.1, vh.0,
            vl.1, l_int, l_frac,
            vm.1, m_int, m_frac,
            vh.1, h_int, h_frac
            );*/
        }
        self
    }

    pub fn run(mut self, _graphics: &mut Graphics) -> (i32, i32) {
        self.cache.rdp.sync_full();

        unsafe {
            self.cache.rdp.commands =
                Some(rdp::swap_commands(self.cache.rdp.commands.take().unwrap()));
            rdp::run_command_buffer();
        }

        (
            self.colored_rect_count as i32,
            self.textured_rect_count as i32,
        )
    }
}
