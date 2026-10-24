// Copyright (C) 2023 The Qt Company Ltd.
// Copyright (C) 2026 HyprV contributors.
// SPDX-License-Identifier: LGPL-3.0-only
//! Qt MultiEffect GPU shader semantics; context-owned programs and sampler.
use crate::skia as sk;
use glow::HasContext;
const VERTEX: &str = r#"
attribute vec4 vertex; attribute vec2 texCoord; uniform vec2 offset; uniform vec2 size; uniform vec4 rect; uniform float shadowScale;
uniform vec2 shadowOffset; uniform mat4 matrix;
varying highp vec2 uv; varying highp vec2 shadowUV;
varying highp vec2 uv0;varying highp vec2 uv1;varying highp vec2 uv2;varying highp vec2 uv3;
void main(){
 gl_Position=matrix*vertex;uv=texCoord;
 shadowUV=(texCoord-shadowOffset)*shadowScale+(1.0-shadowScale)*0.5;
 uv0=texCoord+vec2(offset.x,offset.y*0.33);uv1=texCoord+vec2(offset.x*0.33,-offset.y);
 uv2=texCoord+vec2(-offset.x*0.33,offset.y);uv3=texCoord+vec2(-offset.x,-offset.y*0.33);
}"#;
const DOWN: &str = r#"
precision highp float;
uniform highp sampler2D src0;uniform float flip0;
varying highp vec2 uv0;varying highp vec2 uv1;varying highp vec2 uv2;varying highp vec2 uv3;
vec4 s(vec2 p){return texture2D(src0,vec2(p.x,mix(p.y,1.0-p.y,flip0)));}
void main(){gl_FragColor=(s(uv0)+s(uv1)+s(uv2)+s(uv3))/4.0;}
"#;
const COMPOSE: &str = r#"
precision highp float;
uniform highp sampler2D src0;uniform highp sampler2D src1;uniform highp sampler2D src2;uniform highp sampler2D src3;
uniform highp sampler2D src4;uniform highp sampler2D src5;uniform highp sampler2D src6;
uniform float flip0;uniform float flip1;uniform float flip2;uniform float flip3;
uniform float flip4;uniform float flip5;uniform float flip6;
uniform vec4 blurWeights1;uniform vec2 blurWeights2;uniform vec4 shadowWeights1;uniform vec2 shadowWeights2;
uniform vec4 shadowColor;uniform float shadowEnabled;uniform vec4 colorizationColor;
uniform vec4 maskSpread;uniform float maskEnabled;uniform float maskInverted;
uniform vec3 adjustments;uniform float opacity;
varying highp vec2 uv;varying highp vec2 shadowUV;
vec4 s0(vec2 p){return texture2D(src0,vec2(p.x,mix(p.y,1.0-p.y,flip0)));}
vec4 s1(vec2 p){return texture2D(src1,vec2(p.x,mix(p.y,1.0-p.y,flip1)));}
vec4 s2(vec2 p){return texture2D(src2,vec2(p.x,mix(p.y,1.0-p.y,flip2)));}
vec4 s3(vec2 p){return texture2D(src3,vec2(p.x,mix(p.y,1.0-p.y,flip3)));}
vec4 s4(vec2 p){return texture2D(src4,vec2(p.x,mix(p.y,1.0-p.y,flip4)));}
vec4 s5(vec2 p){return texture2D(src5,vec2(p.x,mix(p.y,1.0-p.y,flip5)));}
void main(){
 vec4 color=s0(uv)*blurWeights1.x+s1(uv)*blurWeights1.y+s2(uv)*blurWeights1.z+s3(uv)*blurWeights1.w+s4(uv)*blurWeights2.x+s5(uv)*blurWeights2.y;
 color.rgb=(color.rgb-0.5*color.a)*(1.0+adjustments.x)+0.5*color.a;
 color.rgb+=adjustments.y*color.a;
 float gray=dot(color.rgb,vec3(0.299,0.587,0.114));
 color.rgb=gray*colorizationColor.rgb*colorizationColor.a+color.rgb*(1.0-colorizationColor.a);
 color.rgb=mix(vec3(gray),color.rgb,1.0+adjustments.z);
 if(shadowEnabled>0.5){
 float shadow=s0(shadowUV).a*shadowWeights1.x+s1(shadowUV).a*shadowWeights1.y+s2(shadowUV).a*shadowWeights1.z+s3(shadowUV).a*shadowWeights1.w+s4(shadowUV).a*shadowWeights2.x+s5(shadowUV).a*shadowWeights2.y;
 float aa=min(1.0,color.a+(1.0-color.a)*(1.0-shadow));color=mix(shadowColor,color,aa);
 }
 if(maskEnabled>0.5){float alphaMask=texture2D(src6,vec2(uv.x,mix(uv.y,1.0-uv.y,flip6))).a;
 float mm=smoothstep(maskSpread.x,maskSpread.y,alphaMask)*smoothstep(maskSpread.z,maskSpread.w,1.0-alphaMask);
 color*=(1.0-maskInverted)*mm+maskInverted*(1.0-mm);}
 gl_FragColor=color*opacity;
}"#;
pub(crate) struct Placement {
 pub size:(i32,i32),pub rect:[f32;4],pub opacity:f32,pub top_left:bool,
 pub scale:[f32;2],pub extent:[f32;2],
}
pub(crate) struct Programs {
    down: std::cell::Cell<Option<glow::NativeProgram>>,
    compose: std::cell::Cell<Option<glow::NativeProgram>>,
    qt_shadow: std::cell::Cell<Option<glow::NativeProgram>>,
    qt_shadow_exact: std::cell::Cell<Option<glow::NativeProgram>>,
    sampler: glow::NativeSampler,
    mask_sampler: std::cell::RefCell<Option<glow::NativeSampler>>,
    buffer: glow::NativeBuffer,
}
unsafe fn program_with_vertex(gl:&glow::Context,vertex:&str,fragment:&str)->Result<glow::NativeProgram,String>{
    let program = gl.create_program()?;
    let result = (|| -> Result<(), String> {
        for (kind, source) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &crate::native_gl::shader_source(gl, source));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let e = gl.get_shader_info_log(shader);
                gl.delete_shader(shader);
                return Err(e);
            }
            gl.attach_shader(program, shader);
            gl.delete_shader(shader);
        }
        gl.bind_attrib_location(program,0,"vertex");
        gl.bind_attrib_location(program,1,"texCoord");
        gl.bind_attrib_location(program,0,"qt_Vertex");gl.bind_attrib_location(program,1,"qt_MultiTexCoord0");
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(program),
        Err(e) => {
            gl.delete_program(program);
            Err(e)
        }
    }
}
impl Programs {
    pub(crate) unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let _timing = crate::diagnostics::scope("effect.gl_compile");
        // Shader variants are independent. In particular the exact Qt shadow
        // is large; compiling it for a generic blur wastes driver heap at idle.
        let sampler = gl.create_sampler()?;
        for key in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
            gl.sampler_parameter_i32(sampler, key, glow::LINEAR as i32)
        }
        for key in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
            gl.sampler_parameter_i32(sampler, key, glow::CLAMP_TO_EDGE as i32)
        }
        let buffer = match gl.create_buffer() {
            Ok(buffer) => buffer,
            Err(error) => { gl.delete_sampler(sampler); return Err(error); }
        };
        Ok(Self { down: Default::default(), compose: Default::default(),
            qt_shadow: Default::default(), qt_shadow_exact: Default::default(),
            sampler, mask_sampler: std::cell::RefCell::new(None), buffer })
    }
    unsafe fn variant(&self, gl: &glow::Context, kind: usize) -> Result<glow::NativeProgram, String> {
        let (slot, vertex, fragment) = match kind {
            0 => (&self.down, VERTEX, DOWN),
            1 => (&self.compose, VERTEX, COMPOSE),
            2 => (&self.qt_shadow, VERTEX, include_str!("qt_gl_shadow.frag")),
            _ => (&self.qt_shadow_exact, include_str!("qt_gl_shadow.vert"), include_str!("qt_gl_shadow_exact.frag")),
        };
        if let Some(program) = slot.get() { return Ok(program); }
        let _timing = crate::diagnostics::scope("effect.gl_compile_variant");
        let program = program_with_vertex(gl, vertex, fragment)?;
        slot.set(Some(program));
        Ok(program)
    }

    pub(crate) unsafe fn draw(
        &self,
        gl: &glow::Context,
        textures: &[(sk::gpu::gl::TextureInfo, sk::gpu::SurfaceOrigin)],
        data: &[f32],
        down: bool,
        placement:Placement,
    ) -> Result<(), String> {
        let Placement{size,rect,opacity,top_left,scale,extent}=placement;
        let qt_shadow = !down
            && data.len() >= 34
            && data[32] == 0.
            && data[33] == 4.
            && data[21] > 0.5
            && data[31] < 0.5;
        let p = if down {
            0
        } else if qt_shadow && textures.iter().take(5).all(|(_,o)|*o==sk::gpu::SurfaceOrigin::TopLeft) {
            3
        } else if qt_shadow {
            2
        } else {
            1
        };
        let p = self.variant(gl, p)?;
        gl.use_program(Some(p));
        let (px,py)=(2./size.0 as f32,2./size.1 as f32);let y=if top_left {-1.}else{1.};
        let matrix=[px*scale[0],0.,0.,0.,0.,-py*y*scale[1],0.,0.,0.,0.,1.,0.,px*rect[0]-1.,(1.-py*rect[1])*y,0.,1.];
        gl.uniform_matrix_4_f32_slice(gl.get_uniform_location(p,"matrix").as_ref(),false,&matrix);
        gl.uniform_matrix_4_f32_slice(gl.get_uniform_location(p,"_19.qt_Matrix").as_ref(),false,&matrix);
        // QSG indexes its quad as 2,0,3,1: the TL-BR diagonal changes UV rounding.
        let vertices=[0f32,extent[1],0.,1.,0.,0.,0.,0.,extent[0],extent[1],1.,1.,extent[0],0.,1.,0.];
        let mut vertex_bytes=[0u8;64];for(i,v)in vertices.into_iter().enumerate(){vertex_bytes[i*4..i*4+4].copy_from_slice(&v.to_ne_bytes());}
        gl.bind_buffer(glow::ARRAY_BUFFER,Some(self.buffer));gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,&vertex_bytes,glow::STREAM_DRAW);
        gl.enable_vertex_attrib_array(0);gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(0,2,glow::FLOAT,false,16,0);gl.vertex_attrib_pointer_f32(1,2,glow::FLOAT,false,16,8);

        gl.uniform_2_f32(
            gl.get_uniform_location(p, "size").as_ref(),
            size.0 as f32,
            size.1 as f32,
        );
        gl.uniform_4_f32(
            gl.get_uniform_location(p, "rect").as_ref(),
            rect[0],
            rect[1],
            rect[2],
            rect[3],
        );
        gl.uniform_1_f32(gl.get_uniform_location(p, "opacity").as_ref(), opacity);
        gl.uniform_1_f32(
            gl.get_uniform_location(p, "_36.qt_Opacity").as_ref(),
            opacity,
        );

        for (i, (t, origin)) in textures.iter().enumerate() {
            gl.active_texture(glow::TEXTURE0 + i as u32);
            gl.bind_texture(
                glow::TEXTURE_2D,
                std::num::NonZeroU32::new(t.id).map(glow::NativeTexture),
            );
            let sampler=if i==6 && data.len()>31 && data[31]>0.5 {
                let cached=*self.mask_sampler.borrow();
                if let Some(sampler)=cached {sampler} else {
                    let sampler=gl.create_sampler()?;
                    for key in [glow::TEXTURE_MIN_FILTER,glow::TEXTURE_MAG_FILTER] {gl.sampler_parameter_i32(sampler,key,glow::NEAREST as i32);}
                    for key in [glow::TEXTURE_WRAP_S,glow::TEXTURE_WRAP_T] {gl.sampler_parameter_i32(sampler,key,glow::CLAMP_TO_EDGE as i32);}
                    *self.mask_sampler.borrow_mut()=Some(sampler);sampler
                }
            } else {self.sampler};
            gl.bind_sampler(i as u32, Some(sampler));
            gl.uniform_1_i32(
                gl.get_uniform_location(p, &format!("src{i}")).as_ref(),
                i as i32,
            );
            gl.uniform_1_i32(
                gl.get_uniform_location(
                    p,
                    &if i == 0 {
                        "src".to_owned()
                    } else {
                        format!("blurSrc{i}")
                    },
                )
                .as_ref(),
                i as i32,
            );

            gl.uniform_1_f32(
                gl.get_uniform_location(p, &format!("flip{i}")).as_ref(),
                if *origin == sk::gpu::SurfaceOrigin::BottomLeft {
                    1.
                } else {
                    0.
                },
            );
        }
        if !down {
            gl.uniform_1_f32(gl.get_uniform_location(p,"_19.shadowScale").as_ref(),data[20]);
            gl.uniform_2_f32(gl.get_uniform_location(p,"_19.shadowOffset").as_ref(),data[18],data[19]);
            gl.uniform_2_f32(gl.get_uniform_location(p,"_19.centerOffset").as_ref(),(1.-data[20])*0.5,(1.-data[20])*0.5);
        }
        if down {
            gl.uniform_2_f32(
                gl.get_uniform_location(p, "offset").as_ref(),
                data[0],
                data[1],
            );
        } else {
            for (name, offset) in [
                ("blurWeights1", 2),
                ("shadowWeights1", 8),
                ("shadowColor", 14),
                ("colorizationColor", 22),
                ("maskSpread", 26),
                ("_36.blurWeight1", 2),
                ("_36.shadowBlurWeight1", 8),
                ("_36.shadowColor", 14),
                ("_36.colorizationColor", 22),
            ] {
                gl.uniform_4_f32(
                    gl.get_uniform_location(p, name).as_ref(),
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                );
            }
            for (name, offset) in [
                ("blurWeights2", 6),
                ("shadowWeights2", 12),
                ("shadowOffset", 18),
                ("_36.blurWeight2", 6),
                ("_36.shadowBlurWeight2", 12),
            ] {
                gl.uniform_2_f32(
                    gl.get_uniform_location(p, name).as_ref(),
                    data[offset],
                    data[offset + 1],
                );
            }
            for (name, offset) in [
                ("shadowScale", 20),
                ("shadowEnabled", 21),
                ("maskInverted", 30),
                ("maskEnabled", 31),
            ] {
                gl.uniform_1_f32(gl.get_uniform_location(p, name).as_ref(), data[offset]);
            }
            for name in ["_36.contrast", "_36.brightness", "_36.saturation"] {
                gl.uniform_1_f32(gl.get_uniform_location(p, name).as_ref(), 0.);
            }
            gl.uniform_3_f32(
                gl.get_uniform_location(p, "adjustments").as_ref(),
                0.,
                0.,
                0.,
            );
        }
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        gl.disable_vertex_attrib_array(1);
        for i in 0..textures.len() {
            gl.bind_sampler(i as u32, None)
        }
        let error = gl.get_error();
        if error != glow::NO_ERROR {
            return Err(format!("effect GL error 0x{error:x}"));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn review_masked_compose(&self) -> bool { self.compose.get().is_some() }
    #[cfg(test)]
    pub(crate) unsafe fn review_lazy_programs(gl: &glow::Context) {
        let effects = Self::new(gl).unwrap();
        assert!(effects.down.get().is_none() && effects.compose.get().is_none()
            && effects.qt_shadow.get().is_none() && effects.qt_shadow_exact.get().is_none());
        let first = effects.variant(gl, 0).unwrap();
        assert!(gl.is_program(first));
        assert_eq!(effects.variant(gl, 0).unwrap(), first);
        assert!(effects.compose.get().is_none() && effects.qt_shadow.get().is_none()
            && effects.qt_shadow_exact.get().is_none(), "downsample must not compile unused shadow/compose variants");
        let exact = effects.variant(gl, 3).unwrap();
        assert!(gl.is_program(exact));
        assert!(effects.compose.get().is_none() && effects.qt_shadow.get().is_none());
        effects.destroy(gl);
        assert!(!gl.is_program(first) && !gl.is_program(exact));
    }
    pub(crate) unsafe fn destroy(&self, gl: &glow::Context) {
        if let Some(program) = self.down.take() { gl.delete_program(program); }
        if let Some(program) = self.compose.take() { gl.delete_program(program); }
        if let Some(program) = self.qt_shadow.take() { gl.delete_program(program); }
        if let Some(program) = self.qt_shadow_exact.take() { gl.delete_program(program); }
        gl.delete_sampler(self.sampler);
        if let Some(sampler)=*self.mask_sampler.borrow(){gl.delete_sampler(sampler);}
        gl.delete_buffer(self.buffer);
    }
}
