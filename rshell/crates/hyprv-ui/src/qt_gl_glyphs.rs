//! Native A8 glyphs packed without resampling; one draw per consecutive atlas page.
//! Integer device quads preserve the original mask shader's pixel coverage.
use crate::{
    native_gl::{Glyph, shader_source},
    skia as sk,
};
use glow::HasContext;
use std::collections::HashMap;
const SIDE: i32 = 512;
const PAGES: usize = 4;
struct Page {
    texture: glow::NativeTexture,
    single_channel: bool,
    x: i32,
    y: i32,
    row: i32,
}
struct Entry {
    _source: sk::Image,
    owners: Vec<u64>,
    page: usize,
    x: i32,
    y: i32,
}
pub(crate) struct Atlas {
    program: glow::NativeProgram,
    owner: u64,
    buffer: glow::NativeBuffer,
    cursor: usize,
    capacity: usize,
    size: Option<glow::NativeUniformLocation>,
    color: Option<glow::NativeUniformLocation>,
    direction: Option<glow::NativeUniformLocation>,
    pages: Vec<Page>,
    entries: HashMap<u32, Entry>,
    vertices: Vec<f32>,
    runs: Vec<(usize, i32, i32)>,
}
impl Atlas {
    pub(crate) fn new(gl: &glow::Context, owner: u64) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let result = (|| {
                for (kind, source) in [
                    (
                        glow::VERTEX_SHADER,
                        "attribute vec2 pos;attribute vec2 texcoord;uniform vec2 size;uniform float direction;varying highp vec2 uv;void main(){gl_Position=vec4(2.0*pos.x/size.x-1.0,(1.0-2.0*pos.y/size.y)*direction,0.0,1.0);uv=texcoord;}",
                    ),
                    (
                        glow::FRAGMENT_SHADER,
                        "precision highp float;uniform highp sampler2D mask;uniform vec4 color;varying highp vec2 uv;void main(){gl_FragColor=color*texture2D(mask,uv).a;}",
                    ),
                ] {
                    let shader = gl.create_shader(kind)?;
                    gl.shader_source(shader, &shader_source(gl, source));
                    gl.compile_shader(shader);
                    if !gl.get_shader_compile_status(shader) {
                        let error = gl.get_shader_info_log(shader);
                        gl.delete_shader(shader);
                        return Err(error);
                    }
                    gl.attach_shader(program, shader);
                    gl.delete_shader(shader);
                }
                gl.bind_attrib_location(program, 0, "pos");
                gl.bind_attrib_location(program, 1, "texcoord");
                gl.link_program(program);
                if !gl.get_program_link_status(program) {
                    return Err(gl.get_program_info_log(program));
                }
                let buffer = gl.create_buffer()?;
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
                gl.buffer_data_size(glow::ARRAY_BUFFER, 256 * 1024, glow::STREAM_DRAW);
                gl.use_program(Some(program));
                gl.uniform_1_i32(gl.get_uniform_location(program, "mask").as_ref(), 0);
                Ok(Self {
                    program,
                    owner,
                    buffer,
                    cursor: 0,
                    capacity: 256 * 1024,
                    size: gl.get_uniform_location(program, "size"),
                    color: gl.get_uniform_location(program, "color"),
                    direction: gl.get_uniform_location(program, "direction"),
                    pages: Vec::new(),
                    entries: HashMap::new(),
                    vertices: Vec::new(),
                    runs: Vec::new(),
                })
            })();
            if result.is_err() {
                gl.delete_program(program);
            }
            result
        }
    }
    #[cfg(test)]
    pub(crate) fn force_next_wrap(&mut self) {
        self.cursor = self.capacity;
    }
    #[cfg(test)]
    pub(crate) fn stats(&self) -> (usize, usize) {
        (
            self.entries.len(),
            self.pages.iter().map(|p| SIDE as usize * SIDE as usize * if p.single_channel {1}else{4}).sum(),
        )
    }
    pub(crate) fn set_resource_owner(&mut self, owner: u64) { self.owner = owner; }
    pub(crate) fn release_owner(&mut self, gl: &glow::Context, owner: u64) {
        self.entries.retain(|_, entry| {
            entry.owners.retain(|id| *id != owner);
            !entry.owners.is_empty()
        });
        // Keep page indices stable for the remaining live glyphs. Empty trailing
        // pages are immediately reusable; shared pages/program/VBO remain warm.
        while !self.pages.is_empty() && !self.entries.values().any(|e| e.page + 1 == self.pages.len()) {
            let page = self.pages.pop().unwrap();
            unsafe { gl.delete_texture(page.texture); }
        }
    }
    pub(crate) fn destroy(self, gl: &glow::Context) {
        unsafe {
            for page in self.pages {
                gl.delete_texture(page.texture);
            }
            gl.delete_buffer(self.buffer);
            gl.delete_program(self.program);
        }
    }
    fn insert(
        &mut self,
        gl: &glow::Context,
        image: &sk::Image,
        before_upload: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<bool, String> {
        let (w, h) = (image.width(), image.height());
        let page = self.pages.iter().position(|p| {
            let y = if p.x + w > SIDE { p.y + p.row } else { p.y };
            y + h <= SIDE
        });
        let index = if let Some(page) = page {
            page
        } else {
            if self.pages.len() == PAGES {
                return Ok(false);
            }
            unsafe {
                let texture = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                for name in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, name, glow::NEAREST as i32);
                }
                for name in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, name, glow::CLAMP_TO_EDGE as i32);
                }
                let single_channel = gl.version().major >= 3;
                if single_channel {
                    // The existing shader reads only .a. Normalized R8 with an
                    // alpha swizzle supplies the identical 8-bit coverage.
                    gl.tex_parameter_i32(glow::TEXTURE_2D,glow::TEXTURE_SWIZZLE_A,glow::RED as i32);
                }
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    if single_channel {glow::R8}else{glow::RGBA} as i32,
                    SIDE,
                    SIDE,
                    0,
                    if single_channel {glow::RED}else{glow::RGBA},
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                self.pages.push(Page {
                    texture,
                    single_channel,
                    x: 0,
                    y: 0,
                    row: 0,
                });
            }
            self.pages.len() - 1
        };
        let page = &mut self.pages[index];
        if page.x + w > SIDE {
            page.x = 0;
            page.y += page.row;
            page.row = 0;
        }
        let mut alpha = vec![0u8; (w * h) as usize];
        if !image.read_pixels(
            &sk::ImageInfo::new((w, h), sk::ColorType::Alpha8, sk::AlphaType::Premul, None),
            &mut alpha,
            w as usize,
            (0, 0),
            sk::image::CachingHint::Allow,
        ) {
            return Err("glyph atlas mask pixels".into());
        }
        before_upload()?;
        let mut rgba = Vec::with_capacity(alpha.len() * 4);
        if !page.single_channel {
            for coverage in &alpha { rgba.extend_from_slice(&[*coverage; 4]); }
        }
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(page.texture));
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                page.x,
                page.y,
                w,
                h,
                if page.single_channel {glow::RED}else{glow::RGBA},
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(if page.single_channel {&alpha}else{&rgba})),
            );
            let error = gl.get_error();
            if error != glow::NO_ERROR {
                return Err(format!("glyph atlas upload GL error {error:#x}"));
            }
        }
        self.entries.insert(
            image.unique_id(),
            Entry {
                _source: image.clone(),
                owners: Vec::new(),
                page: index,
                x: page.x,
                y: page.y,
            },
        );
        page.x += w;
        page.row = page.row.max(h);
        crate::diagnostics::count("native.glyph_atlas_uploads");
        Ok(true)
    }
    pub(crate) fn draw(
        &mut self,
        gl: &glow::Context,
        target: ((i32, i32), u32, bool),
        glyphs: &[Glyph],
        color: [f32; 4],
        clip: sk::IRect,
        mut before_upload: impl FnMut() -> Result<(), String>,
    ) -> Result<bool, String> {
        let (size, fbo, top_left) = target;
        // Oversize/fractional quads use the original renderer, never an
        // approximate atlas placement. Atlas insertion never paints a fragment.
        if glyphs.iter().any(|g| {
            g.image.width() > SIDE
                || g.image.height() > SIDE
                || g.x.fract() != 0.
                || g.y.fract() != 0.
        }) {
            return Ok(false);
        }
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            if gl.version().major >= 3 {
                gl.bind_sampler(0, None);
                gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
                gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
            }
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        }
        self.vertices.clear();
        self.runs.clear();
        for glyph in glyphs {
            let (w, h) = (glyph.image.width() as f32, glyph.image.height() as f32);
            if glyph.x >= clip.right as f32
                || glyph.y >= clip.bottom as f32
                || glyph.x + w <= clip.left as f32
                || glyph.y + h <= clip.top as f32
            {
                continue;
            }
            if !self.entries.contains_key(&glyph.image.unique_id())
                && !self.insert(gl, &glyph.image, &mut before_upload)?
            {
                return Ok(false);
            }
            let entry = self.entries.get_mut(&glyph.image.unique_id()).unwrap();
            if !entry.owners.contains(&self.owner) { entry.owners.push(self.owner); }
            let (x, y) = (glyph.x, glyph.y);
            let (u, v) = (entry.x as f32 / SIDE as f32, entry.y as f32 / SIDE as f32);
            let (uw, vh) = (w / SIDE as f32, h / SIDE as f32);
            let start = (self.vertices.len() / 4) as i32;
            self.vertices.extend_from_slice(&[
                x,
                y,
                u,
                v,
                x,
                y + h,
                u,
                v + vh,
                x + w,
                y,
                u + uw,
                v,
                x + w,
                y,
                u + uw,
                v,
                x,
                y + h,
                u,
                v + vh,
                x + w,
                y + h,
                u + uw,
                v + vh,
            ]);
            if let Some(last) = self.runs.last_mut().filter(|r| r.0 == entry.page) {
                last.2 += 6;
            } else {
                self.runs.push((entry.page, start, 6));
            }
        }
        if self.vertices.is_empty() {
            return Ok(true);
        }
        unsafe {
            gl.bind_framebuffer(
                glow::FRAMEBUFFER,
                std::num::NonZeroU32::new(fbo).map(glow::NativeFramebuffer),
            );
            gl.viewport(0, 0, size.0, size.1);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left,
                if top_left {
                    clip.top
                } else {
                    size.1 - clip.bottom
                },
                clip.width(),
                clip.height(),
            );
            for flag in [glow::STENCIL_TEST, glow::DEPTH_TEST, glow::CULL_FACE] {
                gl.disable(flag);
            }
            gl.enable(glow::DITHER);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.color_mask(true, true, true, true);
            gl.use_program(Some(self.program));
            gl.uniform_2_f32(self.size.as_ref(), size.0 as f32, size.1 as f32);
            gl.uniform_1_f32(self.direction.as_ref(), if top_left { -1. } else { 1. });
            gl.uniform_4_f32(self.color.as_ref(), color[0], color[1], color[2], color[3]);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.buffer));
            let bytes = std::slice::from_raw_parts(
                self.vertices.as_ptr().cast::<u8>(),
                self.vertices.len() * 4,
            );
            if bytes.len() > 1024 * 1024 {
                return Ok(false);
            }
            // Append into unused storage so consecutive text batches do not
            // orphan/allocate one GL buffer per node. Orphan only on wrap.
            if self.cursor + bytes.len() > self.capacity {
                self.capacity = self.capacity.max(bytes.len().next_power_of_two());
                gl.buffer_data_size(glow::ARRAY_BUFFER, self.capacity as i32, glow::STREAM_DRAW);
                self.cursor = 0;
            }
            let offset = self.cursor as i32;
            gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, offset, bytes);
            self.cursor += bytes.len();
            gl.enable_vertex_attrib_array(0);
            gl.enable_vertex_attrib_array(1);
            gl.disable_vertex_attrib_array(7);
            gl.vertex_attrib_1_f32(7, 0.);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, offset);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, offset + 8);
            let error = gl.get_error();
            if error != glow::NO_ERROR {
                return Err(format!("glyph atlas setup GL error {error:#x}"));
            }
            for (page, start, count) in &self.runs {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.pages[*page].texture));
                gl.draw_arrays(glow::TRIANGLES, *start, *count);
                crate::diagnostics::count("native.glyph_atlas_draws");
            }
            gl.disable_vertex_attrib_array(1);
            let error = gl.get_error();
            if error != glow::NO_ERROR {
                return Err(format!("glyph atlas draw GL error {error:#x}"));
            }
        }
        Ok(true)
    }
}
