// Copyright (C) 2016 The Qt Company Ltd.
// Copyright (C) 2026 HyprV contributors.
// SPDX-License-Identifier: LGPL-3.0-only
//! Direct Qt smoothcolor vertex material on the registered EGL target.
//! Geometry is canonical qt_rect; buffers are bounded and context-owned.
use crate::qt_rect::Mesh;
use glow::HasContext;
use std::{collections::HashMap, rc::Rc};
type Key = (usize, bool, [u32;2]);
struct Buffer {
    _mesh: Rc<Mesh>,
    buffer: glow::NativeBuffer,
    indices: glow::NativeBuffer,
    count: i32,
    bytes: usize,
    used: u64,
}
pub(crate) struct Cache {
    program: glow::NativeProgram,
    matrix: glow::NativeUniformLocation,
    pixel_size: glow::NativeUniformLocation,
    opacity: glow::NativeUniformLocation,
    buffers: HashMap<Key, Buffer>,
    clock: u64,
    bytes: usize,
}
impl Cache {
    pub(crate) unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let program = gl.create_program()?;
        let setup = (|| -> Result<_, String> {
            for (kind, source) in [
                (glow::VERTEX_SHADER,include_str!("qt_gl_rect.vert")),
                (glow::FRAGMENT_SHADER,include_str!("qt_gl_rect.frag")),
            ] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader,&crate::native_gl::shader_source(gl,source));
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) { let e=gl.get_shader_info_log(shader); gl.delete_shader(shader); return Err(e) }
                gl.attach_shader(program, shader); gl.delete_shader(shader);
            }
            gl.bind_attrib_location(program, 0, "vertex");
            gl.bind_attrib_location(program, 1, "vertexColor");
            gl.bind_attrib_location(program, 2, "vertexOffset");
            gl.bind_attrib_location(program, 7, "_qt_order");
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            Ok((
                gl.get_uniform_location(program, "_14.matrix")
                    .ok_or("rect matrix")?,
                gl.get_uniform_location(program, "_14.pixelSize")
                    .ok_or("rect pixelSize")?,
                gl.get_uniform_location(program, "_14.opacity")
                    .ok_or("rect opacity")?,
            ))
        })();
        match setup {
            Ok((matrix, pixel_size, opacity)) => Ok(Self {
                program,
                matrix,
                pixel_size,
                opacity,
                buffers: HashMap::new(),
                clock: 0,
                bytes: 0,
            }),
            Err(e) => {
                gl.delete_program(program);
                Err(e)
            }
        }
    }
    pub(crate) unsafe fn draw(
        &mut self,
        gl: &glow::Context,
        size: (i32, i32),
        mesh: Rc<Mesh>,
        geometry: (&[f32;16],[f32;2]),
        opacity: f32,
        aa: bool,
    ) -> Result<(), String> {
        self.clock += 1;
        let (transform,translation)=geometry;
        let key = (Rc::as_ptr(&mesh) as usize, aa, translation.map(f32::to_bits));
        if !self.buffers.contains_key(&key) {
            let mut bytes = Vec::with_capacity(mesh.vertices.len()*20);
            for mut v in mesh.vertices.iter().copied() {
                if !aa {v.offset=[0.,0.];}
                v.position[0]+=translation[0];v.position[1]+=translation[1];
                for p in v.position {bytes.extend_from_slice(&p.to_ne_bytes());}
                bytes.extend_from_slice(&v.color);
                for offset in v.offset {bytes.extend_from_slice(&offset.to_ne_bytes());}
            }
            let index_bytes:Vec<u8>=mesh.indices.iter().flat_map(|i|i.to_ne_bytes()).collect();
            let charge=bytes.len()+index_bytes.len();
            const BUDGET: usize = 8 * 1024 * 1024;
            if charge > BUDGET {
                return Err("rectangle mesh exceeds GPU cache budget".into());
            }
            while self.buffers.len() >= 1024 || self.bytes + charge > BUDGET {
                let key = *self.buffers.iter().min_by_key(|(_, b)| b.used).unwrap().0;
                let old = self.buffers.remove(&key).unwrap();
                self.bytes -= old.bytes;
                gl.delete_buffer(old.buffer);
                gl.delete_buffer(old.indices);
            }
            let buffer = gl.create_buffer()?;
            let indices=match gl.create_buffer(){Ok(b)=>b,Err(e)=>{gl.delete_buffer(buffer);return Err(e)}};
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(indices));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER,&index_bytes,glow::STATIC_DRAW);
            self.bytes += charge;
            self.buffers.insert(
                key,
                Buffer {
                    count: mesh.indices.len() as i32,
                    bytes: charge,
                    _mesh: mesh,
                    buffer,
                    indices,
                    used: self.clock,
                },
            );
        }
        let buffer = self.buffers.get_mut(&key).unwrap();
        buffer.used = self.clock;
        gl.use_program(Some(self.program));
        let (px, py) = (2.0 / size.0 as f32, 2.0 / size.1 as f32);
        gl.uniform_matrix_4_f32_slice(Some(&self.matrix), false, transform);
        gl.uniform_2_f32(Some(&self.pixel_size), px, py);
        gl.uniform_1_f32(Some(&self.opacity), opacity);
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer.buffer));
        gl.enable_vertex_attrib_array(0);
        gl.enable_vertex_attrib_array(1);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 20, 0);
        gl.vertex_attrib_pointer_f32(1, 4, glow::UNSIGNED_BYTE, true, 20, 8);
        gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, 20, 12);
        gl.disable_vertex_attrib_array(7);
        gl.vertex_attrib_1_f32(7,0.);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(buffer.indices));
        gl.enable(glow::DITHER);
        gl.draw_elements(glow::TRIANGLE_STRIP,buffer.count,glow::UNSIGNED_SHORT,0);
        gl.disable_vertex_attrib_array(1);
        gl.disable_vertex_attrib_array(2);
        let error = gl.get_error();
        if error != glow::NO_ERROR {
            return Err(format!("rect GL error 0x{error:x}"));
        }
        Ok(())
    }
    pub(crate) unsafe fn destroy(&mut self, gl: &glow::Context) {
        for (_, b) in self.buffers.drain() {
            gl.delete_buffer(b.buffer);
            gl.delete_buffer(b.indices);
        }
        gl.delete_program(self.program);
    }
}

#[cfg(test)]
impl Cache {
    pub(crate) unsafe fn review_buffers(&self, gl: &glow::Context) -> (usize, usize) {
        let mut bytes = 0;
        for b in self.buffers.values() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(b.buffer));
            bytes += gl.get_buffer_parameter_i32(glow::ARRAY_BUFFER, glow::BUFFER_SIZE) as usize;
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(b.indices));
            bytes += gl.get_buffer_parameter_i32(glow::ELEMENT_ARRAY_BUFFER,glow::BUFFER_SIZE)as usize;
        }
        (self.buffers.len(), bytes)
    }
}
