//! Opt-in UI-thread wall-time profiling. No timers or per-frame file writes.
use std::{cell::RefCell, collections::BTreeMap, fmt::Write, sync::{Mutex, OnceLock}, time::Instant};
#[derive(Default)]
struct Timing {
    count: u64,
    total: u128,
    max: u128,
    cpu_total: u128,
    cpu_max: u128,
}
struct Event {
    name: &'static str,
    namespace: String,
    at: u128,
    cpu_at: u128,
}
struct State {
    epoch: Instant,
    timings: BTreeMap<&'static str, Timing>,
    events: Vec<Event>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            timings: Default::default(),
            events: vec![],
        }
    }
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("HYPRV_UI_PROFILE").is_some())
}
static COUNTS: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());
/// Cross-thread wake-source counts; enabled only for explicit profiling runs.
pub fn count(name: &'static str) {
    count_by(name, 1);
}
pub fn count_by(name: &'static str, value: u64) {
    if enabled() { *COUNTS.lock().unwrap().entry(name).or_default() += value; }
}
fn thread_cpu_ns()->u128 {
    #[repr(C)] struct Time {seconds:std::ffi::c_long,nanos:std::ffi::c_long}
    unsafe extern "C" {fn clock_gettime(id:std::ffi::c_int,out:*mut Time)->std::ffi::c_int;}
    let mut time=Time{seconds:0,nanos:0};
    // Linux CLOCK_THREAD_CPUTIME_ID; never called on the normal release path.
    if unsafe{clock_gettime(3,&mut time)}==0 {time.seconds as u128*1_000_000_000+time.nanos as u128}else{0}
}
pub struct Scope {
    name: &'static str,
    start: Option<Instant>,
    cpu_start:u128,
}
pub fn scope(name: &'static str) -> Scope {
    Scope {
        name,
        start: enabled().then(Instant::now),
        cpu_start:if enabled(){thread_cpu_ns()}else{0},
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let cpu=thread_cpu_ns().saturating_sub(self.cpu_start);
            let ns = start.elapsed().as_nanos();
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                let t = s.timings.entry(self.name).or_default();
                t.count += 1;
                t.total += ns;
                t.max = t.max.max(ns);
                t.cpu_total+=cpu;t.cpu_max=t.cpu_max.max(cpu);
            });
        }
    }
}
pub fn mark(name: &'static str, namespace: &str) {
    if !enabled() {
        return;
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.events.len() < 8192 {
            let at = s.epoch.elapsed().as_nanos();
            s.events.push(Event {
                name,
                namespace: namespace.into(),
                at,
                cpu_at:thread_cpu_ns(),
            });
        }
    });
}
pub fn clear() {
    COUNTS.lock().unwrap().clear();
    STATE.with(|s| *s.borrow_mut() = State::default());
}
pub fn csv() -> String {
    STATE.with(|s| {
        let s = s.borrow();
        let mut out = "kind,name,namespace,at_ns,count,total_ns,max_ns,cpu_total_ns,cpu_max_ns,thread_cpu_ns\n".to_string();
        for (name, t) in &s.timings {
            let _ = writeln!(out, "span,{name},,,{},{},{},{},{},", t.count, t.total, t.max,t.cpu_total,t.cpu_max);
        }
        for (name, count) in COUNTS.lock().unwrap().iter() {
            let _ = writeln!(out, "counter,{name},,,{count},0,0,0,0,");
        }
        for e in &s.events {
            let _ = writeln!(
                out,
                "event,{},{},{},,,,,,{}",
                e.name,
                e.namespace.replace([',', '\n', '\r'], "_"),
                e.at,e.cpu_at
            );
        }
        out
    })
}
pub(crate) fn finish() {
    if enabled() {
        if let Some(path) = std::env::var_os("HYPRV_UI_PROFILE_OUTPUT") {
            let _ = std::fs::write(path, csv());
        }
    }
}
