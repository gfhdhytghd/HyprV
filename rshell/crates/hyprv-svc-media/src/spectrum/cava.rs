//! cava 1.0.0 pipeline for `scripts/audio-spectrum.sh`: the vendored, unmodified
//! `cavacore` (FFI) plus an exact replica of what cava's binary does around it:
//!
//! * `config.c`: `sensitivity` /100, `noise_reduction` /100, `sdl_glsl_gain` 100/100 = 1,
//!   `monstercat` raw (the ×1.5 is applied in the filter). `integral`, `gravity` and
//!   `ignore` from the legacy config are **not parsed** by cava 1.0.0 and have no effect.
//!   No `[input]` section → the PipeWire input (last available default method):
//!   48000 Hz, 16-bit, 2 channels, source `auto`.
//! * `input/common.c` `write_to_cava_input_buffers`: S16 samples stored as raw doubles in
//!   `cava_in`; when the buffer would overflow it is zeroed and restarted.
//! * `cava.c` main loop (one call to [`Cava::frame`] per frame at `framerate`):
//!   `samples_to_use` (not high-framerate at 48 kHz/24 fps → everything buffered),
//!   `cava_execute`, shift, `cava_out *= sens`, ×gain, clamp 0..1, × height
//!   (`ascii_max_range`), split left/right into `float`, `monstercat_filter` per channel
//!   (`max(bar[z] / pow(monstercat*1.5, de), bar[m])`), mono `average` `(l+r)/2` in
//!   `float`, truncation to `int`, and `print_raw_out` clamping to `ascii_max_range`.
use std::{ffi::CStr, os::raw::{c_char, c_double, c_int, c_uint}, sync::Mutex};

#[repr(C)]
struct CavaPlan {
    _opaque: [u8; 0],
}
extern "C" {
    fn cava_init(bars: c_int, rate: c_uint, channels: c_int, autosens: c_int, noise_reduction: c_double, low: c_int, high: c_int) -> *mut CavaPlan;
    fn cava_execute(input: *mut c_double, new_samples: c_int, output: *mut c_double, plan: *mut CavaPlan);
    fn cava_destroy(plan: *mut CavaPlan);
    fn hyprv_cava_status(plan: *const CavaPlan) -> c_int;
    fn hyprv_cava_error(plan: *const CavaPlan) -> *const c_char;
    fn hyprv_cava_input_buffer_size(plan: *const CavaPlan) -> c_int;
}
/// FFTW planning is not thread safe.
static FFTW_PLANNER: Mutex<()> = Mutex::new(());

/// The `audio-spectrum.sh` config after cava's own parsing/validation.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub bars: usize,
    pub framerate: u32,
    pub autosens: bool,
    /// `sensitivity=45` → 0.45
    pub sens: f64,
    pub lower_cut_off: i32,
    pub upper_cut_off: i32,
    /// `noise_reduction=35` → 0.35
    pub noise_reduction: f64,
    pub monstercat: f64,
    /// `ascii_max_range`
    pub range: i32,
    pub rate: u32,
    pub channels: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bars: 16, framerate: 24, autosens: true, sens: 45.0 / 100.0,
            lower_cut_off: 50, upper_cut_off: 12000, noise_reduction: 35.0 / 100.0,
            monstercat: 1.4, range: 100, rate: 48_000, channels: 2,
        }
    }
}

/// `BUFFER_SIZE` in `input/common.h`.
const INPUT_BUFFER_FRAMES: usize = 512;

pub struct Cava {
    config: Config,
    plan: *mut CavaPlan,
    cava_in: Vec<f64>,
    samples_counter: usize,
    cava_out: Vec<f64>,
    /// `audio.input_buffer_size` = BUFFER_SIZE × channels.
    input_buffer_size: usize,
    high_framerate: bool,
    samples_per_frame: usize,
}
// The plan is owned exclusively; cavacore keeps no global state off Android.
unsafe impl Send for Cava {}

impl Cava {
    pub fn new(config: Config) -> Result<Self, String> {
        let plan = {
            let _guard = FFTW_PLANNER.lock().unwrap_or_else(|e| e.into_inner());
            // SAFETY: plain C call; returns an owned plan (never null except on OOM).
            unsafe {
                cava_init(config.bars as c_int, config.rate, config.channels as c_int, config.autosens as c_int,
                    config.noise_reduction, config.lower_cut_off, config.upper_cut_off)
            }
        };
        if plan.is_null() {
            return Err("cava_init failed".into());
        }
        // SAFETY: plan is valid until cava_destroy / free.
        if unsafe { hyprv_cava_status(plan) } != 0 {
            let message = unsafe { CStr::from_ptr(hyprv_cava_error(plan)) }.to_string_lossy().into_owned();
            // A failed init only allocated the struct itself.
            unsafe { libc::free(plan.cast()) };
            return Err(message);
        }
        let cava_buffer_size = unsafe { hyprv_cava_input_buffer_size(plan) }.max(1) as usize;
        let input_buffer_size = INPUT_BUFFER_FRAMES * config.channels;
        let samples_per_frame = (config.rate / config.framerate.max(1)) as usize;
        Ok(Self {
            high_framerate: samples_per_frame < input_buffer_size / config.channels,
            samples_per_frame,
            input_buffer_size,
            cava_in: vec![0.0; cava_buffer_size],
            samples_counter: 0,
            cava_out: vec![0.0; config.bars * config.channels],
            plan,
            config,
        })
    }

    pub fn config(&self) -> &Config { &self.config }

    /// `write_to_cava_input_buffers` for interleaved S16 (its loop counter is uint16_t).
    pub fn push_s16(&mut self, samples: &[i16]) {
        let samples = &samples[..samples.len().min(u16::MAX as usize).min(self.cava_in.len())];
        if samples.is_empty() {
            return;
        }
        if self.samples_counter + samples.len() > self.cava_in.len() {
            self.cava_in.iter_mut().for_each(|v| *v = 0.0);
            self.samples_counter = 0;
        }
        for (i, s) in samples.iter().enumerate() {
            self.cava_in[self.samples_counter + i] = *s as f64;
        }
        self.samples_counter += samples.len();
    }

    /// One iteration of cava's main loop; returns the ascii raw values.
    pub fn frame(&mut self) -> Vec<i32> {
        let channels = self.config.channels;
        let mut samples_to_use = if !self.high_framerate {
            self.samples_counter
        } else {
            let mut n = (self.samples_per_frame * channels).min(self.samples_counter);
            if self.samples_counter > self.input_buffer_size + n {
                n = self.samples_counter - self.input_buffer_size;
            }
            n
        };
        samples_to_use = samples_to_use.min(self.cava_in.len());
        // SAFETY: buffers are sized per cava_init (input ≥ samples_to_use, output bars×channels).
        unsafe { cava_execute(self.cava_in.as_mut_ptr(), samples_to_use as c_int, self.cava_out.as_mut_ptr(), self.plan) };
        self.samples_counter -= samples_to_use;
        if self.samples_counter != 0 {
            self.cava_in.copy_within(samples_to_use..samples_to_use + self.samples_counter, 0);
        }

        let height = self.config.range as f64;
        for v in self.cava_out.iter_mut() {
            *v *= self.config.sens;
            // sdl_glsl_gain = 100/100 = 1.0: `if (gain != 1.0)` skips the multiply.
            *v = v.clamp(0.0, 1.0);
            *v *= height;
        }
        let bars = self.config.bars;
        let mut left: Vec<f32> = self.cava_out[..bars].iter().map(|v| *v as f32).collect();
        let mut right: Vec<f32> = if channels == 2 { self.cava_out[bars..2 * bars].iter().map(|v| *v as f32).collect() } else { Vec::new() };
        if self.config.monstercat > 0.0 {
            monstercat_filter(&mut left, self.config.monstercat);
            if channels == 2 { monstercat_filter(&mut right, self.config.monstercat) }
        }
        let range = self.config.range;
        (0..bars)
            .map(|n| {
                let raw: f32 = if channels == 2 { (left[n] + right[n]) / 2.0 } else { left[n] };
                (raw as i32).min(range) // C float→int truncation, then print_raw_out clamp
            })
            .collect()
    }
}

impl Drop for Cava {
    fn drop(&mut self) {
        let _guard = FFTW_PLANNER.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: plan came from a successful cava_init and is destroyed once.
        unsafe { cava_destroy(self.plan) };
    }
}

/// `cava.c` `monstercat_filter` with waves=0 and height ≤ 1000: the `max` macro compares
/// in double and the result is stored back into the `float` bar.
fn monstercat_filter(bars: &mut [f32], monstercat: f64) {
    let n = bars.len();
    let factor = monstercat * 1.5;
    for z in 0..n {
        for m in (0..z).rev() {
            let a = bars[z] as f64 / factor.powf((z - m) as f64);
            let b = bars[m] as f64;
            bars[m] = if a > b { a } else { b } as f32;
        }
        for m in z + 1..n {
            let a = bars[z] as f64 / factor.powf((m - z) as f64);
            let b = bars[m] as f64;
            bars[m] = if a > b { a } else { b } as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tone(freq: f64, frames: usize, offset: usize) -> Vec<i16> {
        (0..frames).flat_map(|i| {
            let v = (12000.0 * (2.0 * std::f64::consts::PI * freq * (i + offset) as f64 / 48000.0).sin()) as i16;
            [v, v]
        }).collect()
    }
    #[test]
    fn plan_and_mode() {
        let c = Cava::new(Config::default()).unwrap();
        assert!(!c.high_framerate, "48 kHz / 24 fps consumes all buffered samples per frame");
        assert!(c.cava_in.len() >= 4096);
        assert!(Cava::new(Config { channels: 3, ..Config::default() }).is_err());
    }
    #[test]
    fn silence_and_tone() {
        let mut c = Cava::new(Config::default()).unwrap();
        for _ in 0..10 {
            c.push_s16(&[0; 4000]);
            assert_eq!(c.frame(), vec![0; 16]);
        }
        let mut last = Vec::new();
        for f in 0..96 {
            c.push_s16(&tone(2000.0, 2000, f * 2000));
            last = c.frame();
            assert!(last.iter().all(|v| (0..=100).contains(v)));
        }
        let peak = (0..16).max_by_key(|i| last[*i]).unwrap();
        assert!(peak >= 8, "{last:?}");
        assert!(last[peak] > 20, "{last:?}");
    }
    #[test]
    fn overflow_discards_like_cava() {
        let mut c = Cava::new(Config::default()).unwrap();
        let cap = c.cava_in.len();
        c.push_s16(&vec![1; cap - 10]);
        c.push_s16(&[5; 20]);
        assert_eq!(c.samples_counter, 20);
        assert!(c.cava_in[20..].iter().all(|v| *v == 0.0));
    }
    #[test]
    fn monstercat_matches_c_semantics() {
        let mut b = vec![0.0f32, 0.0, 84.0, 0.0];
        monstercat_filter(&mut b, 1.4);
        assert_eq!(b[2], 84.0);
        assert_eq!(b[1], (84.0f64 / (1.4f64 * 1.5)) as f32);
        assert_eq!(b[0], (84.0f64 / (1.4f64 * 1.5).powf(2.0)) as f32);
    }
}
