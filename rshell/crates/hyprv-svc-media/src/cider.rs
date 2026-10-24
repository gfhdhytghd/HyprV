//! Cider compatibility adapter (replaces `scripts/cider-metadata-fallback.py`).
//!
//! Cider publishes incomplete Chromium MPRIS metadata. A lookup queries the local Cider
//! HTTP API (`/api/v2/playback/now-playing`, then v1; 0.7 s timeout per request; first
//! without a token, then each `apptoken`) and falls back to scanning Cider's IndexedDB
//! LevelDB files. Lookups are only issued by the mpris actor (metadata change, plus the
//! 30 s visible+playing fallback). A newer lookup cancels an in-flight one so a stale
//! response can never overwrite another song. Tokens never leave this module.
use crate::common::{action_queue, err, single_line, Context, Handle, Reply};
use hyprv_core::store::TopicKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::{Path, PathBuf}, time::Duration};

pub const DEFAULT_API_BASE: &str = "http://127.0.0.1:10767";
const TIMEOUT: Duration = Duration::from_millis(700);
const ENDPOINTS: [&str; 2] = ["/api/v2/playback/now-playing", "/api/v1/playback/now-playing"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source { Api, Cache }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CiderTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub duration_ms: Option<i64>,
    pub source: Option<Source>,
}
impl CiderTrack {
    fn usable(&self) -> bool {
        !self.title.is_empty() || !self.artist.is_empty() || !self.album.is_empty() || !self.art_url.is_empty()
    }
}

/// `--artist --album --length --current-title` of the legacy helper.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CiderQuery {
    pub artist: String,
    pub album: String,
    pub length_seconds: f64,
    pub current_title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CiderState {
    pub pending: bool,
    pub last_query: Option<CiderQuery>,
    pub last_result: Option<CiderTrack>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CiderAction {
    Lookup(CiderQuery),
}

pub struct CiderTopic;
impl TopicKey for CiderTopic { type Value = CiderState; }
pub type CiderHandle = Handle<CiderState, CiderAction, Option<CiderTrack>>;

/// Configuration resolved from the environment once; token files are re-read per lookup.
#[derive(Clone)]
pub struct CiderConfig {
    pub api_base: String,
    env_tokens: Vec<String>,
    token_files: Vec<PathBuf>,
    indexeddb_dirs: Vec<PathBuf>,
}
impl CiderConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let env_tokens = ["HYPRV_CIDER_APP_TOKEN", "CIDER_APP_TOKEN"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .collect();
        let base = home.join(".config/sh.cider.genten/IndexedDB");
        Self {
            api_base: std::env::var("HYPRV_CIDER_API_BASE").unwrap_or_else(|_| DEFAULT_API_BASE.into()),
            env_tokens,
            token_files: vec![home.join(".config/hyprv/cider-app-token"), home.join(".config/HyprV/.cider-app-token")],
            indexeddb_dirs: vec![
                base.join("http_127.0.0.1_10767.indexeddb.leveldb"),
                base.join("http_localhost_10767.indexeddb.leveldb"),
            ],
        }
    }
    #[cfg(test)]
    pub(crate) fn for_test(api_base: String, indexeddb_dirs: Vec<PathBuf>) -> Self {
        Self { api_base, env_tokens: Vec::new(), token_files: Vec::new(), indexeddb_dirs }
    }
    fn tokens(&self) -> Vec<String> {
        let mut tokens = self.env_tokens.clone();
        for path in &self.token_files {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Some(line) = text.trim().lines().next() {
                    let line = line.trim();
                    if !line.is_empty() { tokens.push(line.to_owned()) }
                }
            }
        }
        let mut unique = Vec::new();
        for t in tokens { if !unique.contains(&t) { unique.push(t) } }
        unique
    }
}

pub fn spawn(ctx: &Context) -> CiderHandle { spawn_with(ctx, CiderConfig::from_env()) }

pub fn spawn_with(ctx: &Context, config: CiderConfig) -> CiderHandle {
    let topic = ctx.store.topic::<CiderTopic>(CiderState::default());
    let (actions, mut queue) = action_queue::<CiderAction, Option<CiderTrack>>();
    let mut shutdown = ctx.runtime.shutdown_signal();
    let publish = topic.clone();
    let task = ctx.runtime.spawn(async move {
        let http = match reqwest::Client::builder().timeout(TIMEOUT).user_agent("HyprV").no_proxy().build() {
            Ok(c) => c,
            Err(_) => return,
        };
        let mut next: Option<(CiderAction, Reply<Option<CiderTrack>>)> = None;
        loop {
            let (CiderAction::Lookup(query), reply) = match next.take() {
                Some(r) => r,
                None => tokio::select! {
                    r = queue.recv() => match r { Some(r) => r, None => break },
                    _ = shutdown.changed() => break,
                },
            };
            publish.publish(CiderState { pending: true, last_query: Some(query.clone()), last_result: publish.snapshot().value.last_result });
            let lookup_query = query.clone();
            let lookup = find_matching_track(&http, &config, &lookup_query);
            tokio::pin!(lookup);
            tokio::select! {
                track = &mut lookup => {
                    publish.publish(CiderState { pending: false, last_query: Some(query), last_result: track.clone() });
                    let _ = reply.send(Ok(track));
                }
                newer = queue.recv() => {
                    let _ = reply.send(err("superseded by a newer lookup"));
                    match newer { Some(r) => next = Some(r), None => break }
                }
                _ = shutdown.changed() => { let _ = reply.send(err("shutdown")); break }
            }
        }
    });
    Handle::new(topic, actions, task)
}

pub async fn find_matching_track(http: &reqwest::Client, config: &CiderConfig, query: &CiderQuery) -> Option<CiderTrack> {
    let target_ms = (query.length_seconds * 1000.0).round_ties_even();
    if !target_ms.is_finite() || target_ms <= 0.0 {
        return None;
    }
    let target_ms = target_ms as i64;
    if let Some(track) = find_api_track(http, config, target_ms, &query.current_title).await {
        return Some(track);
    }
    let (dirs, base) = (config.indexeddb_dirs.clone(), config.api_base.clone());
    let query = query.clone();
    tokio::task::spawn_blocking(move || find_cache_track(&dirs, &base, &query.artist, &query.album, target_ms, &query.current_title))
        .await
        .ok()
        .flatten()
}

async fn find_api_track(http: &reqwest::Client, config: &CiderConfig, target_ms: i64, current_title: &str) -> Option<CiderTrack> {
    let mut attempts = vec![String::new()];
    attempts.extend(config.tokens());
    let base = config.api_base.trim_end_matches('/');
    for endpoint in ENDPOINTS {
        let url = format!("{base}{endpoint}");
        let mut matches: Vec<(i64, CiderTrack)> = Vec::new();
        for token in &attempts {
            let mut request = http.get(&url);
            if !token.is_empty() {
                request = request.header("apptoken", token);
            }
            let Ok(response) = request.send().await else { continue };
            if !response.status().is_success() { continue }
            let Ok(bytes) = response.bytes().await else { continue };
            let Ok(payload) = serde_json::from_slice::<Value>(String::from_utf8_lossy(&bytes).as_bytes()) else { continue };
            if let Some(track) = api_payload_track(&payload, &config.api_base, target_ms, current_title, &mut matches) {
                return Some(track);
            }
            if !matches.is_empty() { break }
        }
        if !matches.is_empty() {
            matches.sort_by_key(|m| m.0);
            return matches.into_iter().next().map(|m| m.1);
        }
    }
    None
}

/// One API response: `data` object wins; otherwise score every nested mapping.
fn api_payload_track(payload: &Value, base: &str, target_ms: i64, current_title: &str, matches: &mut Vec<(i64, CiderTrack)>) -> Option<CiderTrack> {
    if let Some(data) = payload.get("data").filter(|d| d.is_object()) {
        let mut track = track_from_mapping(data, base);
        if track.usable() {
            track.source = Some(Source::Api);
            return Some(track);
        }
    }
    let mut mappings = Vec::new();
    candidate_mappings(payload, &mut mappings);
    for (index, mapping) in mappings.into_iter().enumerate() {
        let mut track = track_from_mapping(mapping, base);
        if !track.usable() { continue }
        if let Some(score) = requested_track_score(&track, "", "", target_ms, current_title, index as i64) {
            track.source = Some(Source::Api);
            matches.push((score, track));
        }
    }
    None
}

fn candidate_mappings<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            out.push(value);
            for v in map.values() { candidate_mappings(v, out) }
        }
        Value::Array(items) => for v in items { candidate_mappings(v, out) },
        _ => {}
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
/// Python `a or b or ...` over optional JSON values.
fn first<'a>(candidates: &[Option<&'a Value>]) -> Option<&'a Value> {
    candidates.iter().flatten().copied().find(|v| truthy(v))
}
fn text(v: Option<&Value>) -> String {
    match v {
        None => String::new(),
        Some(Value::String(s)) => single_line(s),
        Some(other) => single_line(&other.to_string()),
    }
}
fn number(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

pub fn format_artwork_url(value: &str, base: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let url = value.replace("{w}", "512").replace("{h}", "512").replace("{f}", "jpg").replace("{c}", "bb");
    if url.starts_with("//") {
        format!("https:{url}")
    } else if url.starts_with('/') {
        format!("{}{url}", base.trim_end_matches('/'))
    } else {
        url
    }
}

pub fn track_from_mapping(data: &Value, base: &str) -> CiderTrack {
    let Some(map) = data.as_object() else { return CiderTrack::default() };
    let attributes = map.get("attributes").filter(|a| a.as_object().is_some_and(|o| !o.is_empty()));
    let source = attributes.unwrap_or(data);
    let s = |k: &str| source.get(k);
    let d = |k: &str| data.get(k);
    let mut artwork = s("artwork").cloned();
    if let Some(Value::Object(o)) = &artwork {
        artwork = o.get("url").cloned();
    } else if !artwork.as_ref().is_some_and(truthy) {
        if let Some(Value::Object(o)) = d("artwork") {
            artwork = o.get("url").cloned();
        }
    }
    let duration = number(first(&[s("durationInMillis"), s("duration_ms"), s("durationMs"), s("duration")]));
    let duration = duration.map(|n| if n > 0.0 && n < 10_000.0 { n * 1000.0 } else { n });
    let art = first(&[artwork.as_ref(), s("artworkUrl"), s("artworkURL"), d("artworkUrl"), d("artworkURL")]);
    CiderTrack {
        title: text(first(&[s("name"), s("title"), d("name"), d("title")])),
        artist: text(first(&[s("artistName"), s("artist"), d("artistName"), d("artist")])),
        album: text(first(&[s("albumName"), s("album"), d("albumName"), d("album")])),
        art_url: format_artwork_url(&art.map(|v| match v { Value::String(s) => s.clone(), o => o.to_string() }).unwrap_or_default(), base),
        duration_ms: duration.filter(|d| d.is_finite()).map(|d| d.round_ties_even() as i64),
        source: None,
    }
}

pub fn normalized(value: &str) -> String {
    value.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}
fn artist_matches(candidate: &str, requested: &str) -> bool {
    let (c, r) = (normalized(candidate), normalized(requested));
    r.is_empty() || c == r || r.contains(&c) || c.contains(&r)
}
pub fn is_generic_title(value: &str) -> bool {
    matches!(normalized(value).as_str(), "" | "cider" | "chromium")
}

pub fn requested_track_score(track: &CiderTrack, artist: &str, album: &str, target_ms: i64, current_title: &str, rank: i64) -> Option<i64> {
    let mut score = rank;
    match track.duration_ms {
        Some(d) if target_ms > 0 && d != 0 => {
            let delta = (d - target_ms).abs();
            if delta > 2500 { return None }
            score += delta;
        }
        _ if target_ms > 0 => score += 5000,
        _ => {}
    }
    if !artist.is_empty() && !artist_matches(&track.artist, artist) { return None }
    if !album.is_empty() && normalized(&track.album) != normalized(album) { return None }
    if !current_title.is_empty() && !is_generic_title(current_title) {
        if normalized(&track.title) == normalized(current_title) { score -= 3000 } else { score += 1200 }
    }
    if !track.title.is_empty() { score -= 100 }
    if !track.artist.is_empty() { score -= 80 }
    if !track.art_url.is_empty() { score -= 60 }
    Some(score)
}

// ---- IndexedDB (LevelDB) structured-clone scan, cider-metadata-fallback.py:15-73,296-366

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { out.push(byte | 0x80) } else { out.push(byte); return out }
    }
}
fn read_varint(data: &[u8], offset: usize) -> Option<u64> {
    let mut value = 0u64;
    for i in 0..10 {
        let byte = *data.get(offset + i)?;
        value |= ((byte & 0x7f) as u64) << (7 * i);
        if byte & 0x80 == 0 { return Some(value) }
    }
    None
}
fn varint_end(data: &[u8], offset: usize) -> Option<usize> {
    (0..10).find(|i| data.get(offset + i).is_some_and(|b| b & 0x80 == 0)).map(|i| offset + i + 1)
}
fn key_pattern(key: &str, tag: u8) -> Vec<u8> {
    let mut p = vec![b'"'];
    p.extend(encode_varint(key.len() as u64));
    p.extend(key.as_bytes());
    p.push(tag);
    p
}
fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() { return None }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|i| i + from)
}
fn read_string_field(window: &[u8], key: &str) -> String {
    let pattern = key_pattern(key, b'"');
    let Some(index) = find(window, &pattern, 0) else { return String::new() };
    let at = index + pattern.len();
    let (Some(length), Some(start)) = (read_varint(window, at), varint_end(window, at)) else { return String::new() };
    if length > 4096 || start + length as usize > window.len() { return String::new() }
    String::from_utf8(window[start..start + length as usize].to_vec()).unwrap_or_default()
}
fn read_integer_field(window: &[u8], key: &str) -> Option<i64> {
    let pattern = key_pattern(key, b'I');
    let index = find(window, &pattern, 0)?;
    read_varint(window, index + pattern.len()).map(|raw| (raw >> 1) as i64)
}

pub fn find_cache_track(dirs: &[PathBuf], base: &str, artist: &str, album: &str, target_ms: i64, current_title: &str) -> Option<CiderTrack> {
    let mut matches: Vec<(i64, i64, i64, CiderTrack)> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if !path.is_file() || !matches!(ext, Some("log" | "ldb")) { continue }
            scan_file(&path, base, artist, album, target_ms, current_title, &mut matches);
        }
    }
    matches.sort_by_key(|m| (m.0, m.1, m.2));
    matches.into_iter().next().map(|m| m.3)
}

fn scan_file(path: &Path, base: &str, artist: &str, album: &str, target_ms: i64, current_title: &str, matches: &mut Vec<(i64, i64, i64, CiderTrack)>) {
    let Ok(data) = std::fs::read(path) else { return };
    let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    let pattern = key_pattern("durationInMillis", b'I');
    let mut offset = 0;
    while let Some(index) = find(&data, &pattern, offset) {
        offset = index + 1;
        let window = &data[index.saturating_sub(1400)..(index + 1800).min(data.len())];
        let Some(duration) = read_integer_field(window, "durationInMillis") else { continue };
        if (duration - target_ms).abs() > 1500 { continue }
        let art = [read_string_field(window, "artwork"), read_string_field(window, "artworkUrl"), read_string_field(window, "artworkURL")]
            .into_iter().find(|s| !s.is_empty()).unwrap_or_default();
        let track = CiderTrack {
            title: single_line(&read_string_field(window, "name")),
            artist: single_line(&read_string_field(window, "artistName")),
            album: single_line(&read_string_field(window, "albumName")),
            art_url: format_artwork_url(&art, base),
            duration_ms: Some(duration),
            source: Some(Source::Cache),
        };
        if !track.usable() { continue }
        if let Some(score) = requested_track_score(&track, artist, album, target_ms, current_title, 0) {
            matches.push((score, -mtime, -(index as i64), track));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const BASE: &str = "http://127.0.0.1:10767";

    #[test]
    fn artwork_formatting() {
        assert_eq!(format_artwork_url("//x/{w}x{h}{c}.{f}", BASE), "https://x/512x512bb.jpg");
        assert_eq!(format_artwork_url("/art.png", BASE), "http://127.0.0.1:10767/art.png");
        assert_eq!(format_artwork_url("", BASE), "");
    }
    #[test]
    fn mapping_prefers_attributes_and_python_or() {
        let t = track_from_mapping(&json!({
            "name": "outer", "attributes": {"name": "", "title": "Inner\nSong", "artistName": "A", "albumName": "B",
            "artwork": {"url": "//img/{w}"}, "durationInMillis": 0, "duration": "215.5"}
        }), BASE);
        assert_eq!(t.title, "Inner Song");
        assert_eq!((t.artist.as_str(), t.album.as_str()), ("A", "B"));
        assert_eq!(t.art_url, "https://img/512");
        assert_eq!(t.duration_ms, Some(215_500));
        let t = track_from_mapping(&json!({"title": "x", "artwork": {"url": "/a"}, "durationInMillis": 200000}), BASE);
        assert_eq!((t.art_url.as_str(), t.duration_ms), ("http://127.0.0.1:10767/a", Some(200000)));
        let t = track_from_mapping(&json!({"artwork": "", "artworkUrl": "u"}), BASE);
        assert_eq!(t.art_url, "u");
        assert_eq!(track_from_mapping(&json!([1]), BASE), CiderTrack::default());
    }
    #[test]
    fn scoring_rules() {
        let t = CiderTrack { title: "Song".into(), artist: "Band".into(), duration_ms: Some(200_000), ..Default::default() };
        assert_eq!(requested_track_score(&t, "", "", 201_000, "", 0), Some(1000 - 180));
        assert_eq!(requested_track_score(&t, "", "", 203_000, "", 0), None);
        assert_eq!(requested_track_score(&t, "band feat x", "", 200_000, "", 0), Some(-180));
        assert_eq!(requested_track_score(&t, "Other", "", 200_000, "", 0), None);
        assert_eq!(requested_track_score(&t, "", "", 200_000, "song", 0), Some(-3180));
        assert_eq!(requested_track_score(&t, "", "", 200_000, "Cider", 0), Some(-180));
        assert_eq!(requested_track_score(&t, "", "", 200_000, "Else", 0), Some(1020));
        let no_dur = CiderTrack { title: "x".into(), ..Default::default() };
        assert_eq!(requested_track_score(&no_dur, "", "", 1, "", 2), Some(4902));
    }
    #[test]
    fn api_payload_shapes() {
        let mut m = Vec::new();
        let direct = api_payload_track(&json!({"data": {"name": "Now", "artistName": "Me"}}), BASE, 1000, "", &mut m).unwrap();
        assert_eq!((direct.title.as_str(), direct.source), ("Now", Some(Source::Api)));
        let nested = json!({"info": [{"name": "Far", "durationInMillis": 9000000}, {"name": "Near", "durationInMillis": 200500}]});
        assert!(api_payload_track(&nested, BASE, 200_000, "", &mut m).is_none());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].1.title, "Near");
    }
    #[test]
    fn varints_and_cache_scan() {
        for v in [0u64, 1, 127, 128, 300, 1 << 40] {
            let e = encode_varint(v);
            assert_eq!(read_varint(&e, 0), Some(v));
            assert_eq!(varint_end(&e, 0), Some(e.len()));
        }
        let dir = std::env::temp_dir().join(format!("hyprv-cider-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut blob = b"junk".to_vec();
        let string_field = |k: &str, v: &str, blob: &mut Vec<u8>| {
            blob.extend(key_pattern(k, b'"'));
            blob.extend(encode_varint(v.len() as u64));
            blob.extend(v.as_bytes());
        };
        string_field("name", "Cached Song", &mut blob);
        string_field("artistName", "Cached Artist", &mut blob);
        string_field("artwork", "//c/{w}", &mut blob);
        blob.extend(key_pattern("durationInMillis", b'I'));
        blob.extend(encode_varint(200_400 << 1));
        std::fs::write(dir.join("000003.log"), &blob).unwrap();
        std::fs::write(dir.join("ignored.txt"), &blob).unwrap();
        let t = find_cache_track(std::slice::from_ref(&dir), BASE, "cached artist", "", 200_000, "Cider").unwrap();
        assert_eq!((t.title.as_str(), t.art_url.as_str(), t.source), ("Cached Song", "https://c/512", Some(Source::Cache)));
        assert!(find_cache_track(std::slice::from_ref(&dir), BASE, "", "", 100_000, "").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn http_lookup_against_fake_server() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { break };
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let body = if req.starts_with("GET /api/v2") { "{}".to_string() } else {
                    json!({"data": {"attributes": {"name": "From API", "artistName": "X", "artwork": {"url": "/img"}}}}).to_string()
                };
                let resp = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
                let _ = s.write_all(resp.as_bytes()).await;
            }
        });
        let base = format!("http://{addr}");
        let config = CiderConfig::for_test(base.clone(), vec![]);
        let http = reqwest::Client::builder().timeout(TIMEOUT).no_proxy().build().unwrap();
        let q = CiderQuery { length_seconds: 200.0, current_title: "Chromium".into(), ..Default::default() };
        let t = find_matching_track(&http, &config, &q).await.unwrap();
        assert_eq!(t.title, "From API");
        assert_eq!(t.art_url, format!("{base}/img"));
        assert!(find_matching_track(&http, &config, &CiderQuery::default()).await.is_none());
    }
}
