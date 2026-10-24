//! Qt Canvas atlas allocation/sampling. ScreenCornerShade is black, so R8 plus
//! (0,0,0,R) swizzle preserves its sampled RGBA using one quarter of RGBA8 storage.
use crate::skia as sk;
use glow::HasContext;

pub(crate) unsafe fn upload(
    gl: &glow::Context,
    image: &sk::Image,
    size: (u32, u32),
) -> Result<glow::NativeTexture, String> {
    let side = image.width() as usize;
    let vertical = size.0 >= size.1;
    if gl.version().major < 3
        || image.height() != image.width() * 4
        || image.color_type() != sk::ColorType::Alpha8
        || size.0 < (side as u32 + 2) * if vertical { 1 } else { 4 }
        || size.1 < (side as u32 + 2) * if vertical { 4 } else { 1 }
        || size.0 > gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) as u32
        || size.1 > gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) as u32
    {
        return Err("invalid Qt Canvas A8 atlas".into());
    }
    let map = image
        .peek_pixels()
        .ok_or("Canvas atlas must be raster backed")?;
    let source = map.bytes().ok_or("Canvas atlas pixels")?;
    let texture = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    for p in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
        gl.tex_parameter_i32(glow::TEXTURE_2D, p, glow::LINEAR as i32);
    }
    for p in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
        gl.tex_parameter_i32(glow::TEXTURE_2D, p, glow::CLAMP_TO_EDGE as i32);
    }
    for p in [
        glow::TEXTURE_SWIZZLE_R,
        glow::TEXTURE_SWIZZLE_G,
        glow::TEXTURE_SWIZZLE_B,
    ] {
        gl.tex_parameter_i32(glow::TEXTURE_2D, p, glow::ZERO as i32);
    }
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_SWIZZLE_A, glow::RED as i32);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::R8 as i32,
        size.0 as i32,
        size.1 as i32,
        0,
        glow::RED,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
    );
    let old_alignment = gl.get_parameter_i32(glow::UNPACK_ALIGNMENT);
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    let edge = side + 2;
    let mut padded = vec![0; edge * edge];
    for part in 0..4 {
        for y in 0..edge {
            for x in 0..edge {
                padded[y * edge + x] = source[(part * side + y.saturating_sub(1).min(side - 1))
                    * map.row_bytes()
                    + x.saturating_sub(1).min(side - 1)];
            }
        }
        let (x, y) = if vertical {
            (0, part * edge)
        } else {
            (part * edge, 0)
        };
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            x as i32,
            y as i32,
            edge as i32,
            edge as i32,
            glow::RED,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&padded)),
        );
    }
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, old_alignment);
    let error = gl.get_error();
    if error != glow::NO_ERROR {
        gl.delete_texture(texture);
        return Err(format!("Qt Canvas atlas upload GL error {error:#x}"));
    }
    Ok(texture)
}

pub(crate) fn uv(size: (u32, u32), side: i32, corner: u8) -> [f32; 4] {
    let offset = corner as f32 * (side + 2) as f32;
    let (x, y) = if size.0 >= size.1 {
        (1., 1. + offset)
    } else {
        (1. + offset, 1.)
    };
    [
        x / size.0 as f32,
        y / size.1 as f32,
        side as f32 / size.0 as f32,
        side as f32 / size.1 as f32,
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atlas_uvs_match_original_qsg_padding_and_allocation() {
        assert_eq!(
            uv((8192, 4096), 54, 2),
            [1. / 8192., 113. / 4096., 54. / 8192., 54. / 4096.]
        );
        assert_eq!(
            uv((4096, 8192), 54, 3),
            [169. / 4096., 1. / 8192., 54. / 4096., 54. / 8192.]
        );
    }
}
