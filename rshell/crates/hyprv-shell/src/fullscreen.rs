//! Compose actual GPU surface captures in layer/creation order and apply parity gates.
use hyprv_ui::{
    skia as sk,
    wayland::{Anchor, Layer, LayerConfig, SurfaceCapture},
};
use serde_json::{json, Value};
use std::error::Error;
use std::path::Path;
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}
impl Image {
    pub fn transparent(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
        }
    }
    pub fn load(path: &Path) -> Result<Self, Box<dyn Error>> {
        Self::decode(&std::fs::read(path)?)
    }
    pub(crate) fn decode(bytes:&[u8]) -> Result<Self, Box<dyn Error>> {
        let image = sk::Image::from_encoded(sk::Data::new_copy(bytes))
            .ok_or("invalid PNG")?;
        let (width, height) = (image.width() as usize, image.height() as usize);
        let info = sk::ImageInfo::new(
            (width as i32, height as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let mut out = Self::transparent(width, height);
        if !image.read_pixels(
            &info,
            &mut out.pixels,
            width * 4,
            (0, 0),
            sk::image::CachingHint::Allow,
        ) {
            return Err("PNG readback failed".into());
        }
        Ok(out)
    }
    pub fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        let info = sk::ImageInfo::new(
            (self.width as i32, self.height as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let image =
            sk::images::raster_from_data(&info, sk::Data::new_copy(&self.pixels), self.width * 4)
                .ok_or("image allocation")?;
        let data = image
            .encode(None, sk::EncodedImageFormat::PNG, None)
            .ok_or("PNG encode")?;
        std::fs::write(path, data.as_bytes())?;
        Ok(())
    }
    pub fn over(&mut self, source: &Self, x: i32, y: i32) {
        for sy in 0..source.height {
            let dy = y + sy as i32;
            if dy < 0 || dy >= self.height as i32 {
                continue;
            }
            for sx in 0..source.width {
                let dx = x + sx as i32;
                if dx < 0 || dx >= self.width as i32 {
                    continue;
                }
                let s = &source.pixels[(sy * source.width + sx) * 4..][..4];
                let d = &mut self.pixels[(dy as usize * self.width + dx as usize) * 4..][..4];
                for c in 0..4 {
                    d[c] = (s[c] as u32 + (d[c] as u32 * (255 - s[3] as u32) + 127) / 255).min(255)
                        as u8;
                }
            }
        }
    }
}
pub fn origin(config: &LayerConfig, size: (f32, f32), output: (f32, f32)) -> (f32, f32) {
    let a = config.anchor;
    let (t, r, b, l) = config.margins;
    let x = if a.contains(Anchor::LEFT) {
        l as f32
    } else if a.contains(Anchor::RIGHT) {
        output.0 - size.0 - r as f32
    } else {
        ((output.0 - size.0) / 2.).round() + l as f32 - r as f32
    };
    let y = if a.contains(Anchor::TOP) {
        t as f32
    } else if a.contains(Anchor::BOTTOM) {
        output.1 - size.1 - b as f32
    } else {
        ((output.1 - size.1) / 2.).round() + t as f32 - b as f32
    };
    (x, y)
}
pub fn layer(layer: Layer) -> u8 {
    match layer {
        Layer::Background => 0,
        Layer::Bottom => 1,
        Layer::Top => 2,
        Layer::Overlay => 3,
        _ => 4,
    }
}
pub fn compose(
    directory: &Path,
    captures: &[SurfaceCapture],
    output: (f32, f32),
    scale: f64,
    output_name: &str,
) -> Result<Image, Box<dyn Error>> {
    let mut result = Image::transparent(
        (output.0 as f64 * scale).round() as usize,
        (output.1 as f64 * scale).round() as usize,
    );
    if captures.iter().any(|c| !c.config.input_only && c.output.is_none()) {
        return Err("captured surface lacks native output placement".into());
    }
    let mut ordered: Vec<_> = captures
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.config.input_only && c.output.as_deref() == Some(output_name))
        .collect();
    ordered.sort_by_key(|(i, c)| (layer(c.config.layer), *i));
    for (_, capture) in ordered {
        if (capture.scale - scale).abs() > 0.0001 {
            return Err(format!(
                "surface {} DPR {} does not match requested {}",
                capture.config.namespace, capture.scale, scale
            )
            .into());
        }
        let source = Image::load(&directory.join(&capture.file))?;
        let position = origin(
            &capture.config,
            (capture.size.0 as f32, capture.size.1 as f32),
            output,
        );
        result.over(
            &source,
            (position.0 as f64 * scale).round() as i32,
            (position.1 as f64 * scale).round() as i32,
        );
    }
    Ok(result)
}
fn footprint(image: &Image) -> Option<[usize; 4]> {
    let (mut l, mut t, mut r, mut b) = (image.width, image.height, 0, 0);
    let mut painted = false;
    for y in 0..image.height {
        for x in 0..image.width {
            if image.pixels[(y * image.width + x) * 4 + 3] > 0 {
                painted = true;
                l = l.min(x);
                t = t.min(y);
                r = r.max(x + 1);
                b = b.max(y + 1);
            }
        }
    }
    painted.then_some([l, t, r, b])
}
pub fn compare(actual: &Image, reference: &Image, df: bool, scale: f64) -> Value {
    if (actual.width, actual.height) != (reference.width, reference.height) {
        return json!({"status":"size_mismatch","pass":false,"actual":[actual.width,actual.height],"reference":[reference.width,reference.height]});
    }
    let (mut max, mut sum, mut painted, mut union, mut intersection, mut different) =
        (0u8, 0u64, 0u64, 0u64, 0u64, 0u64);
    for (a, b) in actual
        .pixels
        .chunks_exact(4)
        .zip(reference.pixels.chunks_exact(4))
    {
        let p = a[3] > 0 || b[3] > 0;
        let u = a[3] > 0 && b[3] > 0;
        union += u64::from(p);
        intersection += u64::from(u);
        let mut diff = false;
        for c in 0..4 {
            let d = a[c].abs_diff(b[c]);
            max = max.max(d);
            if p {
                sum += d as u64;
            }
            diff |= d != 0;
        }
        if p {
            painted += 1;
        }
        different += u64::from(diff);
    }
    let mean = sum as f64 / (painted.max(1) * 4) as f64;
    let iou = if union == 0 {
        1.
    } else {
        intersection as f64 / union as f64
    };
    let ab = footprint(actual);
    let rb = footprint(reference);
    let extent_ok = match (ab, rb) {
        (None, None) => true,
        (Some(a), Some(b)) => a
            .into_iter()
            .zip(b)
            .all(|(a, b)| a.abs_diff(b) as f64 <= scale),
        _ => false,
    };
    let pass = extent_ok
        && iou >= 0.98
        && if df {
            max <= 8 && mean <= 0.5
        } else {
            max == 0
        };
    json!({"status":if pass{"pass"}else{"fail"},"pass":pass,"premultiplied":true,"max_channel_diff":max,"painted_mae":mean,"painted_iou":iou,"footprint_within_one_logical_pixel":extent_ok,"different_pixels":different,"distance_field_text":df})
}
pub fn reference_entry(
    directory: &Path,
    fixture: &str,
    frame: &str,
    theme: &str,
    scale: f64,
    output: &str,
) -> Result<Option<Value>, Box<dyn Error>> {
    let path = directory.join("manifest.json");
    if !path.exists() {
        return Ok(None);
    }
    let m: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(m["images"]
        .as_array()
        .or_else(|| m["entries"].as_array())
        .into_iter()
        .flatten()
        .find(|e| {
            (e["state"].as_str() == Some(fixture)
                || e["fixture"].as_str().is_some_and(|f| {
                    Path::new(f).file_stem().and_then(|v| v.to_str()) == Some(fixture)
                }))
                && e["frame"].as_str().unwrap_or("") == frame
                && e["theme"] == theme
                && e["scale"]
                    .as_f64()
                    .is_some_and(|s| (s - scale).abs() < 0.0001)
                && e["output"].as_str().or_else(|| e["output"]["name"].as_str()).is_none_or(|o| o == output)
        })
        .cloned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alpha_composition_clips_and_matches_gpu_byte_rounding() {
        let mut dst = Image::transparent(2, 1);
        dst.pixels = [10, 20, 30, 255, 0, 0, 0, 0].to_vec();
        let src = Image {
            width: 1,
            height: 1,
            pixels: vec![50, 25, 0, 128],
        };
        dst.over(&src, 0, 0);
        assert_eq!(&dst.pixels[..4], &[55, 35, 15, 255]);
        dst.over(&src, -1, 0);
        assert_eq!(&dst.pixels[..4], &[55, 35, 15, 255]);
    }
    #[test]
    fn gates_never_accept_non_df_rounding_or_structural_loss() {
        let mut a = Image::transparent(2, 2);
        a.pixels[3] = 255;
        let mut b = Image {
            width: 2,
            height: 2,
            pixels: a.pixels.clone(),
        };
        assert_eq!(compare(&a, &b, false, 2.)["pass"], true);
        b.pixels[0] = 1;
        assert_eq!(compare(&a, &b, false, 2.)["pass"], false);
        assert_eq!(compare(&a, &b, true, 2.)["pass"], true);
        b.pixels[3] = 0;
        assert_eq!(compare(&a, &b, true, 2.)["pass"], false);
    }
}

#[cfg(test)]
mod output_tests {
    use super::*;
    #[test]
    fn composition_selects_output_before_dpr_and_image_validation() {
        let dir=std::env::temp_dir().join(format!("hyprv-fullscreen-output-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Image { width:1,height:1,pixels:vec![255,0,0,255] }.save(&dir.join("red.png")).unwrap();
        let mut captures=vec![SurfaceCapture {
            file:"missing-other-output.png".into(), config:LayerConfig::default(),
            size:(2,2),scale:2.,output:Some("B".into()),
        }, SurfaceCapture {
            file:"red.png".into(),config:LayerConfig::default(),
            size:(1,1),scale:1.,output:Some("A".into()),
        }];
        assert_eq!(compose(&dir,&captures,(1.,1.),1.,"A").unwrap().pixels,[255,0,0,255]);
        captures[1].output=None;
        assert!(compose(&dir,&captures,(1.,1.),1.,"A").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn reference_object_output_is_matched_as_strictly_as_string_output() {
        let dir=std::env::temp_dir().join(format!("hyprv-fullscreen-ref-output-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"),serde_json::to_vec(&json!({"images":[
            {"state":"idle","theme":"dark","scale":1,"output":{"name":"A"},"file":"a.png"},
            {"state":"idle","theme":"dark","scale":1,"output":{"name":"B"},"file":"b.png"}
        ]})).unwrap()).unwrap();
        assert_eq!(reference_entry(&dir,"idle","","dark",1.,"B").unwrap().unwrap()["file"],"b.png");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
