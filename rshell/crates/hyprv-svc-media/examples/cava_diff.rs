//! Parity check: feed identical S16LE/48 kHz/stereo PCM to the installed `cava` (FIFO
//! input, the `audio-spectrum.sh` config otherwise unchanged) and to `spectrum::cava`,
//! then diff every frame. Dev tool only — the shell never runs cava.
//!
//! `cargo run --example cava_diff -- <pcm.s16le> <scratch-dir> [frames]`
//!
//! Lockstep: after cava prints frame k we write chunk k (2048 samples = two FIFO reads of
//! cava's 1024-sample input buffer); cava's next frame (≈41 ms later) consumes exactly that
//! chunk because at 48 kHz/24 fps it is not in high-framerate mode. Our runner replays the
//! same sequence: frame 1 empty, then push chunk k, frame k+1.
use hyprv_svc_media::spectrum::cava::{Cava, Config};
use std::{io::{BufRead, BufReader, Write}, process::{Command, Stdio}};

const CHUNK: usize = 2048;

fn main() {
    let mut args = std::env::args().skip(1);
    let pcm_path = args.next().expect("pcm path");
    let dir = std::path::PathBuf::from(args.next().expect("scratch dir"));
    let frames: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(1000);
    let bytes = std::fs::read(&pcm_path).unwrap();
    let pcm: Vec<i16> = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    let chunks: Vec<&[i16]> = pcm.chunks_exact(CHUNK).take(frames).collect();

    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("cava.fifo");
    let _ = std::fs::remove_file(&fifo);
    let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0, "mkfifo");
    let config = dir.join("cava.conf");
    // Verbatim scripts/audio-spectrum.sh:10-31, plus an [input] section for the FIFO.
    std::fs::write(&config, format!(
        "[general]\nframerate=24\nbars=16\nautosens=1\nsensitivity=45\nlower_cutoff_freq=50\nhigher_cutoff_freq=12000\n\n\
         [input]\nmethod=fifo\nsource={}\nsample_rate=48000\nsample_bits=16\nchannels=2\n\n\
         [output]\nmethod=raw\nraw_target=/dev/stdout\ndata_format=ascii\nascii_max_range=100\nchannels=mono\nmono_option=average\n\n\
         [smoothing]\nnoise_reduction=35\nintegral=88\ngravity=92\nignore=2\nmonstercat=1.4\n",
        fifo.display()
    )).unwrap();

    let mut child = Command::new("cava").arg("-p").arg(&config)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::inherit())
        .spawn().expect("cava");
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut writer = std::fs::OpenOptions::new().write(true).open(&fifo).unwrap();

    let mut theirs: Vec<Vec<i32>> = Vec::new();
    for k in 0..=chunks.len() {
        let line = lines.next().expect("cava frame").unwrap();
        theirs.push(line.split(';').filter(|s| !s.is_empty()).map(|s| s.parse().unwrap()).collect());
        if let Some(chunk) = chunks.get(k) {
            let raw: Vec<u8> = chunk.iter().flat_map(|s| s.to_le_bytes()).collect();
            writer.write_all(&raw).unwrap();
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&fifo);

    let mut ours = Cava::new(Config::default()).unwrap();
    let mut mine = vec![ours.frame()];
    for chunk in &chunks {
        for half in chunk.chunks(1024) { ours.push_s16(half) }
        mine.push(ours.frame());
    }

    let mut mismatched = 0;
    let mut max_delta = 0;
    let mut nonzero = 0;
    for (k, (a, b)) in theirs.iter().zip(&mine).enumerate() {
        if a.iter().any(|v| *v != 0) { nonzero += 1 }
        if a != b {
            mismatched += 1;
            let d = a.iter().zip(b).map(|(x, y)| (x - y).abs()).max().unwrap_or(i32::MAX);
            max_delta = max_delta.max(d);
            if mismatched <= 5 { eprintln!("frame {k}: cava {a:?}\n          ours {b:?}") }
        }
    }
    println!("frames compared: {}  non-silent: {nonzero}  mismatched: {mismatched}  max |delta|: {max_delta}", theirs.len());
    std::process::exit(i32::from(mismatched != 0));
}
