//! Explicit private-fixture capture only. Match the original reference harness's
//! black/white Hyprland matte, including buffer filtering and RGB blend rounding.
//! No production caller, watcher, timer or service helper is installed.
use crate::fullscreen::Image;
use std::{
    error::Error,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::Command,
    time::Duration,
};

fn private_socket() -> Result<PathBuf, Box<dyn Error>> {
    let runtime = PathBuf::from(std::env::var("XDG_RUNTIME_DIR")?);
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")?;
    let name = runtime.file_name().and_then(|v| v.to_str()).unwrap_or("");
    if std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref() != Ok("1")
        || std::env::var_os("HYPRV_NESTED_FIXTURE").is_none()
        || runtime.parent() != Some(std::env::temp_dir().as_path())
        || !name.starts_with("hv-")
        || signature.contains('/')
        || signature.is_empty()
        || std::env::var("DBUS_SYSTEM_BUS_ADDRESS")? != std::env::var("DBUS_SESSION_BUS_ADDRESS")?
    {
        return Err("compositor capture requires the isolated native fixture driver".into());
    }
    Ok(runtime.join("hypr").join(signature).join(".socket.sock"))
}
fn background(socket: &std::path::Path, color: &str) -> Result<(), Box<dyn Error>> {
    if !matches!(color, "0xff000000" | "0xffffffff") {
        return Err("invalid private matte color".into());
    }
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream
        .write_all(format!("eval hl.config({{misc={{background_color={color}}}}})").as_bytes())?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    if reply.trim() != "ok" {
        return Err(format!("private background change failed: {reply}").into());
    }
    Ok(())
}
fn grab(output: &str, size: (usize, usize)) -> Result<Image, Box<dyn Error>> {
    let reply = Command::new("grim")
        .args(["-t", "png", "-o", output, "-"])
        .output()?;
    if !reply.status.success() {
        return Err(format!(
            "private screencopy failed: {}",
            String::from_utf8_lossy(&reply.stderr)
        )
        .into());
    }
    let image = Image::decode(&reply.stdout)?;
    if (image.width, image.height) != size {
        return Err("private screencopy extent mismatch".into());
    }
    Ok(image)
}
fn settled(output: &str, size: (usize, usize)) -> Result<Image, Box<dyn Error>> {
    let mut previous = grab(output, size)?;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        let next = grab(output, size)?;
        if previous.pixels == next.pixels {
            return Ok(next);
        }
        previous = next;
    }
    Err("private compositor did not settle at the frozen fixture clock".into())
}
pub(crate) fn capture(output: &str, size: (usize, usize)) -> Result<Image, Box<dyn Error>> {
    let socket = private_socket()?;
    for _ in 0..3 {
        background(&socket, "0xff000000")?;
        let black = settled(output, size)?;
        background(&socket, "0xffffffff")?;
        let white = settled(output, size);
        // Restore even if screencopy fails. This is always a verified private socket.
        let restore = background(&socket, "0xff000000");
        let white = white?;
        restore?;
        let again = settled(output, size)?;
        if black.pixels != again.pixels {
            continue;
        }
        return matte(&black, &white);
    }
    Err("assembled output changed during all three private mattes".into())
}
fn matte(black: &Image, white: &Image) -> Result<Image, Box<dyn Error>> {
    if (black.width, black.height) != (white.width, white.height) {
        return Err("matte extent mismatch".into());
    }
    let mut out = Image::transparent(black.width, black.height);
    let (mut inconsistent, mut spread_max) = (0usize, 0i16);
    for ((b, w), o) in black
        .pixels
        .chunks_exact(4)
        .zip(white.pixels.chunks_exact(4))
        .zip(out.pixels.chunks_exact_mut(4))
    {
        let mut alpha = [0i16; 3];
        for c in 0..3 {
            if w[c] < b[c] {
                return Err("white matte is darker than black".into());
            }
            alpha[c] = 255 - (w[c] as i16 - b[c] as i16);
        }
        alpha.sort_unstable();
        let spread = alpha[2] - alpha[0];
        spread_max = spread_max.max(spread);
        if spread > 2 || b[..3].iter().any(|v| *v as i16 > alpha[1] + 2) {
            inconsistent += 1;
        }
        let a = alpha[1].max(*b[..3].iter().max().unwrap() as i16);
        o[..3].copy_from_slice(&b[..3]);
        o[3] = a as u8;
    }
    // Same capture-validity checks as parity/fullscreen/tools/run.py. These do
    // not relax the independent pixel acceptance gate or exclude any pixel.
    if inconsistent > 64 || spread_max > 8 {
        return Err("inconsistent compositor matte".into());
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matte_preserves_rounded_compositor_alpha_including_empty_fringe() {
        let black = Image {
            width: 2,
            height: 1,
            pixels: vec![0, 0, 0, 255, 10, 20, 30, 255],
        };
        let white = Image {
            width: 2,
            height: 1,
            pixels: vec![251, 251, 251, 255, 138, 148, 158, 255],
        };
        assert_eq!(
            matte(&black, &white).unwrap().pixels,
            vec![0, 0, 0, 4, 10, 20, 30, 127]
        );
    }
    #[test]
    fn changing_content_cannot_be_a_matte() {
        let black = Image {
            width: 1,
            height: 1,
            pixels: vec![100, 0, 0, 255],
        };
        let white = Image {
            width: 1,
            height: 1,
            pixels: vec![99, 0, 0, 255],
        };
        assert!(matte(&black, &white).is_err());
    }
}
