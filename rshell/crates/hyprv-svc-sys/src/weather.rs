//! OpenWeather compatibility, retained caches and visible-only freshness deadlines.
use crate::support::*;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate};
use futures_util::StreamExt;
use hyprv_core::store::TopicKey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
    Kelvin,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherHour {
    pub local_time: String,
    pub temperature: f64,
    pub icon: String,
    pub color: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherDay {
    pub id: String,
    pub local_date: String,
    pub weekday: String,
    pub high: f64,
    pub low: f64,
    pub feels_like: f64,
    pub wind: f64,
    pub humidity_percent: f64,
    pub precipitation_percent: f64,
    pub icon: String,
    pub color: String,
    pub description: String,
    pub hours: Vec<WeatherHour>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub location: Option<String>,
    pub settings_generation: u64,
    pub data_generation: u64,
    pub unit: TemperatureUnit,
    pub current_temperature: Option<f64>,
    pub current_icon: String,
    pub current_color: String,
    pub days: Vec<WeatherDay>,
    pub dummy: bool,
    pub stale: bool,
    pub selected_day: u8,
    pub last_error: Option<Error>,
}
impl Model for State {
    fn failure(&mut self, e: Error) {
        self.stale = true;
        self.last_error = Some(e);
    }
    fn action_failure(&mut self, error: Error) {
        self.last_error = Some(error);
    }
}
impl State {
    /// Exact string-valued schema consumed by moon.qml:116 and weather.sh.
    pub fn legacy_json(&self) -> Value {
        json!({"current_temp":format!("{:.1}",self.current_temperature.unwrap_or_default()),"current_icon":self.current_icon,"current_hex":self.current_color,"forecast":self.days.iter().map(|d|{let date=NaiveDate::parse_from_str(&d.local_date,"%Y-%m-%d").ok();json!({"id":d.id,"day":date.map(|d|d.format("%a").to_string()).unwrap_or_else(||d.weekday.clone()),"day_full":d.weekday,"date":date.map(|d|d.format("%d %b").to_string()).unwrap_or_default(),"max":format!("{:.1}",d.high),"min":format!("{:.1}",d.low),"feels_like":format!("{:.1}",d.feels_like),"wind":format!("{:.0}",d.wind),"humidity":format!("{:.0}",d.humidity_percent),"pop":format!("{:.0}",d.precipitation_percent),"icon":d.icon,"hex":d.color,"desc":d.description,"hourly":d.hours.iter().map(|h|json!({"time":h.local_time,"temp":format!("{:.1}",h.temperature),"icon":h.icon,"hex":h.color})).collect::<Vec<_>>()})}).collect::<Vec<_>>()})
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Refresh,
    Day(u8),
    Next,
    Previous,
}
#[derive(Clone, Debug)]
pub struct Config {
    pub env_file: PathBuf,
    pub cache_dir: PathBuf,
    pub api_base: String,
}
impl Default for Config {
    fn default() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        Self {
            env_file: home
                .join(".config/HyprV/quickshell/hypr-comp/scripts/quickshell/calendar/.env"),
            cache_dir: PathBuf::from(std::env::var_os("QS_CACHE_WEATHER").unwrap_or_else(|| {
                std::env::var_os("QS_CACHE_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".cache/hyprv/hypr-comp"))
                    .join("weather")
                    .into_os_string()
            })),
            api_base: "https://api.openweathermap.org/data/2.5".into(),
        }
    }
}
pub struct Key;
impl TopicKey for Key {
    type Value = State;
}
#[derive(PartialEq)]
struct Settings {
    key: String,
    city: String,
    units: String,
}
fn settings(path: &Path) -> Result<Settings> {
    let mut values = BTreeMap::new();
    if path.exists() {
        for pair in dotenvy::from_path_iter(path)
            .map_err(|_| Error::new("config", "invalid weather .env"))?
        {
            let (k, v) = pair.map_err(|_| Error::new("config", "invalid weather setting"))?;
            values.insert(k, v);
        }
    }
    let get = |k: &str| {
        values
            .get(k)
            .cloned()
            .or_else(|| std::env::var(k).ok())
            .unwrap_or_default()
    };
    let units = get("OPENWEATHER_UNIT");
    Ok(Settings {
        key: get("OPENWEATHER_KEY"),
        city: get("OPENWEATHER_CITY_ID"),
        units: if ["metric", "imperial", "standard"].contains(&units.as_str()) {
            units
        } else {
            "metric".into()
        },
    })
}
fn missing(s: &Settings) -> bool {
    s.key.is_empty() || s.key == "Skipped" || s.key == "OPENWEATHER_KEY"
}
fn style(code: &str) -> (&'static str, &'static str) {
    match code {
        "50d" | "50n" => ("󰖑", "#84afdb"),
        "01d" => ("", "#f9e2af"),
        "01n" => ("", "#cba6f7"),
        "02d" | "02n" | "03d" | "03n" | "04d" | "04n" => ("", "#bac2de"),
        "09d" | "09n" | "10d" | "10n" => ("󰖗", "#74c7ec"),
        "11d" | "11n" => ("", "#f9e2af"),
        "13d" | "13n" => ("", "#cdd6f4"),
        _ => ("", "#cdd6f4"),
    }
}
fn number(v: &Value) -> Result<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .filter(|v| v.is_finite())
        .ok_or_else(|| Error::new("schema", "weather number missing"))
}
fn dummy(unit: TemperatureUnit, today: NaiveDate) -> State {
    let days = (0..5)
        .map(|i| {
            let d = today + Days::new(i);
            WeatherDay {
                id: i.to_string(),
                local_date: d.to_string(),
                weekday: d.format("%A").to_string(),
                high: 0.,
                low: 0.,
                feels_like: 0.,
                wind: 0.,
                humidity_percent: 0.,
                precipitation_percent: 0.,
                icon: "".into(),
                color: "#cdd6f4".into(),
                description: "No API Key".into(),
                hours: vec![WeatherHour {
                    local_time: "00:00".into(),
                    temperature: 0.,
                    icon: "".into(),
                    color: "#cdd6f4".into(),
                }],
            }
        })
        .collect();
    State {
        unit,
        current_temperature: Some(0.),
        current_icon: "".into(),
        current_color: "#cdd6f4".into(),
        days,
        dummy: true,
        stale: false,
        ..State::default()
    }
}
fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn normalize(forecast: &Value, current: &Value, unit: TemperatureUnit) -> Result<State> {
    let list = forecast["list"]
        .as_array()
        .ok_or_else(|| Error::new("schema", "forecast list missing"))?;
    if list.len() > 512 {
        return Err(Error::new("limit", "forecast too large"));
    }
    // Preserve source grouping by UTC dt_txt date, with *local* hourly labels.
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for item in list {
        let date = item["dt_txt"]
            .as_str()
            .and_then(|s| s.split_once(' '))
            .map(|p| p.0)
            .ok_or_else(|| Error::new("schema", "dt_txt missing"))?;
        NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|e| Error::new("date", e))?;
        groups.entry(date.into()).or_default().push(item);
    }
    let mut days = vec![];
    for (index, (date, items)) in groups.into_iter().take(5).enumerate() {
        let aggregate = |field: &str| max_min(&items, field);
        let high = aggregate("temp_max")?.1;
        let low = aggregate("temp_min")?.0;
        let feels_like = aggregate("feels_like")?.1;
        let humidity = items
            .iter()
            .map(|v| number(&v["main"]["humidity"]))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .sum::<f64>()
            / items.len() as f64;
        let wind = items
            .iter()
            .map(|v| number(&v["wind"]["speed"]))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max)
            .round();
        let pop = items
            .iter()
            .map(|v| v["pop"].as_f64().unwrap_or_default())
            .fold(0., f64::max);
        let middle = items[items.len() / 2];
        let (icon, color) = style(middle["weather"][0]["icon"].as_str().unwrap_or_default());
        let parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d").unwrap();
        let hours = items
            .iter()
            .map(|v| {
                let dt = v["dt"]
                    .as_i64()
                    .and_then(|ts| DateTime::from_timestamp(ts, 0))
                    .ok_or_else(|| Error::new("date", "forecast timestamp missing"))?;
                let (icon, color) = style(v["weather"][0]["icon"].as_str().unwrap_or_default());
                Ok(WeatherHour {
                    local_time: dt.with_timezone(&Local).format("%H:%M").to_string(),
                    temperature: number(&v["main"]["temp"])?,
                    icon: icon.into(),
                    color: color.into(),
                })
            })
            .collect::<Result<_>>()?;
        days.push(WeatherDay {
            id: index.to_string(),
            local_date: date,
            weekday: parsed.format("%A").to_string(),
            high,
            low,
            feels_like,
            wind,
            humidity_percent: humidity.round(),
            precipitation_percent: (pop * 100.).trunc(),
            icon: icon.into(),
            color: color.into(),
            description: title_case(
                middle["weather"][0]["description"]
                    .as_str()
                    .unwrap_or_default(),
            ),
            hours,
        });
    }
    let (icon, color) = style(current["weather"][0]["icon"].as_str().unwrap_or_default());
    Ok(State {
        unit,
        current_temperature: Some(number(&current["main"]["temp"])?),
        current_icon: icon.into(),
        current_color: color.into(),
        days,
        dummy: false,
        stale: false,
        ..State::default()
    })
}
fn max_min(items: &[&Value], field: &str) -> Result<(f64, f64)> {
    let values = items
        .iter()
        .map(|v| number(&v["main"][field]))
        .collect::<Result<Vec<_>>>()?;
    Ok((
        values.iter().copied().fold(f64::INFINITY, f64::min),
        values.into_iter().fold(f64::NEG_INFINITY, f64::max),
    ))
}
fn read_json(path: &Path) -> Result<Value> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(Error::new("cache", "weather cache too large"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn atomic(path: &Path, v: &Value) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new("path", "cache parent absent"))?;
    std::fs::create_dir_all(parent)?;
    let temp = path.with_extension("hyprv-tmp");
    std::fs::write(&temp, serde_json::to_vec(v)?)?;
    std::fs::rename(temp, path)?;
    Ok(())
}
fn import(v: &Value, unit: TemperatureUnit, today: NaiveDate) -> Result<State> {
    let forecast = v["forecast"]
        .as_array()
        .ok_or_else(|| Error::new("cache", "forecast missing"))?;
    let mut state = State {
        unit,
        current_temperature: Some(number(&v["current_temp"])?),
        current_icon: v["current_icon"].as_str().unwrap_or("").into(),
        current_color: v["current_hex"].as_str().unwrap_or("#cdd6f4").into(),
        ..State::default()
    };
    for (index, d) in forecast.iter().take(5).enumerate() {
        let hours = d["hourly"]
            .as_array()
            .ok_or_else(|| Error::new("cache", "hourly missing"))?
            .iter()
            .take(48)
            .map(|h| {
                Ok(WeatherHour {
                    local_time: h["time"].as_str().unwrap_or_default().into(),
                    temperature: number(&h["temp"])?,
                    icon: h["icon"].as_str().unwrap_or_default().into(),
                    color: h["hex"].as_str().unwrap_or_default().into(),
                })
            })
            .collect::<Result<_>>()?;
        state.days.push(WeatherDay {
            id: d["id"].as_str().unwrap_or_default().into(),
            local_date: {
                let text = d["date"].as_str().unwrap_or_default();
                ((today.year() - 1)..=(today.year() + 1))
                    .filter_map(|year| {
                        NaiveDate::parse_from_str(&format!("{year} {text}"), "%Y %d %b").ok()
                    })
                    .min_by_key(|date| date.signed_duration_since(today).num_days().abs())
                    .unwrap_or(today + Days::new(index as u64))
                    .to_string()
            },
            weekday: d["day_full"].as_str().unwrap_or_default().into(),
            high: number(&d["max"])?,
            low: number(&d["min"])?,
            feels_like: number(&d["feels_like"])?,
            wind: number(&d["wind"])?,
            humidity_percent: number(&d["humidity"])?,
            precipitation_percent: number(&d["pop"])?,
            icon: d["icon"].as_str().unwrap_or_default().into(),
            color: d["hex"].as_str().unwrap_or_default().into(),
            description: d["desc"].as_str().unwrap_or_default().into(),
            hours,
        });
    }
    state.dummy = state.days.iter().any(|d| d.description == "No API Key");
    Ok(state)
}
struct Native {
    state: State,
    config: Config,
    settings: Option<Settings>,
    watch: Option<FileWatch>,
    http: reqwest::Client,
    visible: bool,
    sleeping: bool,
    deadline: Option<tokio::time::Instant>,
    refreshed: Option<SystemTime>,
    failures: u32,
    sleep_events: Option<zbus::MessageStream>,
    configuration_dirty: bool,
}
impl Native {
    fn unit(&self) -> TemperatureUnit {
        match self.settings.as_ref().unwrap().units.as_str() {
            "imperial" => TemperatureUnit::Fahrenheit,
            "standard" => TemperatureUnit::Kelvin,
            _ => TemperatureUnit::Celsius,
        }
    }
    fn arm(&mut self) {
        let ttl = if self.state.dummy { 3600 } else { 900 };
        let age = self
            .refreshed
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .unwrap_or(Duration::from_secs(ttl));
        self.state.stale = age >= Duration::from_secs(ttl);
        self.deadline = if self.visible && !self.sleeping {
            Some(tokio::time::Instant::now() + Duration::from_secs(ttl).saturating_sub(age))
        } else {
            None
        };
    }
    fn update_settings(&mut self, next: Settings) -> bool {
        let changed = self.settings.as_ref() != Some(&next);
        if changed {
            self.configuration_dirty = self.settings.is_some();
            self.state.settings_generation = self.state.settings_generation.saturating_add(1);
            self.settings = Some(next);
            self.refreshed = None;
            self.state.stale = true;
        }
        changed
    }
    fn cache(&mut self) {
        let path = self.config.cache_dir.join("weather.json");
        let stamp = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok();
        let changed = stamp != self.refreshed;
        if changed {
            if let Ok(value) = read_json(&path) {
                // Legacy numeric strings contain no unit or city. Do not guess
                // their identity when importing; a successful fetch migrates it.
                if let (Ok(unit), Some(city)) = (
                    serde_json::from_value::<TemperatureUnit>(value["_hyprv"]["unit"].clone()),
                    value["_hyprv"]["city"].as_str(),
                ) {
                    let matching =
                        unit == self.unit() && city == self.settings.as_ref().unwrap().city;
                    if let Ok(mut state) = import(&value, unit, Local::now().date_naive()) {
                        state.selected_day = self.state.selected_day;
                        state.location = Some(city.into());
                        state.settings_generation = self.state.settings_generation;
                        state.data_generation = if matching && !self.configuration_dirty {
                            self.state.settings_generation
                        } else {
                            self.state.data_generation
                        };
                        self.state = state;
                        self.refreshed = if matching && !self.configuration_dirty {
                            stamp
                        } else {
                            None
                        };
                    }
                } else {
                    self.refreshed = None;
                    self.state.stale = true;
                }
            }
        }
        if let Ok(view) = std::fs::read_to_string(self.config.cache_dir.join("view_id")) {
            if let Ok(day) = view.trim().parse::<u8>() {
                self.state.selected_day = day.min(4);
            }
        }
        if changed {
            self.arm();
        }
    }
    async fn get(&self, endpoint: &str) -> Result<Value> {
        let s = self.settings.as_ref().unwrap();
        let response = self
            .http
            .get(format!("{}/{endpoint}", self.config.api_base))
            .query(&[
                ("APPID", s.key.as_str()),
                ("id", s.city.as_str()),
                ("units", s.units.as_str()),
            ])
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|_| Error::new("http", "OpenWeather request failed"))?
            .error_for_status()
            .map_err(|e| {
                Error::new(
                    if e.status()
                        .is_some_and(|s| s.as_u16() == 401 || s.as_u16() == 403)
                    {
                        "key"
                    } else {
                        "http"
                    },
                    "OpenWeather rejected request",
                )
            })?;
        let mut bytes = vec![];
        let mut body = response.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|_| Error::new("http", "weather response interrupted"))?;
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(Error::new("frame", "weather response too large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    async fn refresh(&mut self) -> Result<()> {
        let today = Local::now().date_naive();
        let selected = self.state.selected_day;
        let result: Result<State> = async {
            if missing(self.settings.as_ref().unwrap()) {
                return Ok(dummy(self.unit(), today));
            }
            let mut forecast = self.get("forecast").await?;
            let current = self.get("weather").await?;
            if forecast["cod"].as_str() != Some("200") && forecast["cod"].as_i64() != Some(200) {
                return Err(Error::new("provider", "forecast error"));
            }
            let items = forecast["list"]
                .as_array()
                .ok_or_else(|| Error::new("schema", "forecast list missing"))?;
            let key = today.to_string();
            let tomorrow = (today + Days::new(1)).to_string();
            let mut merged: BTreeMap<i64, Value> = BTreeMap::new();
            // Rollover tomorrow cache before merging today's new slots.
            let reusable_cache = !self.configuration_dirty
                && self.state.location.as_deref()
                    == Some(self.settings.as_ref().unwrap().city.as_str())
                && self.state.unit == self.unit();
            if reusable_cache {
                for name in ["next_day_precache.json", "daily_weather_cache.json"] {
                    if let Ok(cache) = read_json(&self.config.cache_dir.join(name)) {
                        if let Some(items) = cache.as_array() {
                            for v in items {
                                if v["dt_txt"].as_str().is_some_and(|s| s.starts_with(&key)) {
                                    if let Some(dt) = v["dt"].as_i64() {
                                        merged.entry(dt).or_insert_with(|| v.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for v in items {
                if v["dt_txt"].as_str().is_some_and(|s| s.starts_with(&key)) {
                    if let Some(dt) = v["dt"].as_i64() {
                        merged.entry(dt).or_insert_with(|| v.clone());
                    }
                }
            }
            let today_items: Vec<_> = merged.into_values().collect();
            let tomorrow_items: Vec<_> = items
                .iter()
                .filter(|v| {
                    v["dt_txt"]
                        .as_str()
                        .is_some_and(|s| s.starts_with(&tomorrow))
                })
                .cloned()
                .collect();
            let mut all = today_items.clone();
            all.extend(
                items
                    .iter()
                    .filter(|v| !v["dt_txt"].as_str().is_some_and(|s| s.starts_with(&key)))
                    .cloned(),
            );
            forecast["list"] = json!(all);
            let state = normalize(&forecast, &current, self.unit())?;
            atomic(
                &self.config.cache_dir.join("daily_weather_cache.json"),
                &json!(today_items),
            )?;
            atomic(
                &self.config.cache_dir.join("next_day_precache.json"),
                &json!(tomorrow_items),
            )?;
            Ok(state)
        }
        .await;
        match result {
            Ok(mut state) => {
                state.selected_day = selected;
                state.settings_generation = self.state.settings_generation;
                state.data_generation = state.settings_generation;
                state.location = Some(self.settings.as_ref().unwrap().city.clone());
                let mut cache = state.legacy_json();
                cache["_hyprv"] = json!({"unit":state.unit,"city":state.location});
                atomic(&self.config.cache_dir.join("weather.json"), &cache)?;
                self.configuration_dirty = false;
                self.state = state;
                self.refreshed = std::fs::metadata(self.config.cache_dir.join("weather.json"))
                    .and_then(|metadata| metadata.modified())
                    .ok();
                self.failures = 0;
                self.arm();
                Ok(())
            }
            Err(e) => {
                if self.state.days.is_empty() {
                    self.state = dummy(self.unit(), today);
                }
                self.state.stale = true;
                self.state.last_error = Some(e.clone());
                self.failures = self.failures.saturating_add(1);
                self.deadline = if self.visible && !self.sleeping {
                    Some(
                        tokio::time::Instant::now()
                            + Duration::from_secs(if e.code == "key" {
                                3600
                            } else {
                                (30u64 << self.failures.min(6)).min(3600)
                            }),
                    )
                } else {
                    None
                };
                Err(e)
            }
        }
    }
}
#[async_trait::async_trait]
impl Backend for Native {
    type State = State;
    type Action = Action;
    fn state(&self) -> &State {
        &self.state
    }
    fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }
    async fn connect(&mut self) -> Result<()> {
        let paths = vec![
            self.config.env_file.clone(),
            self.config.cache_dir.join("weather.json"),
            self.config.cache_dir.join("view_id"),
        ];
        self.watch = Some(FileWatch::new(paths)?);
        self.update_settings(settings(&self.config.env_file)?);
        self.cache();
        if self.state.days.is_empty() && missing(self.settings.as_ref().unwrap()) {
            let generation = self.state.settings_generation;
            self.state = dummy(self.unit(), Local::now().date_naive());
            self.state.settings_generation = generation;
            self.state.data_generation = generation;
            self.refreshed = Some(SystemTime::now());
            self.arm();
        }
        let connection = zbus::Connection::system().await?;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.login1")?
            .interface("org.freedesktop.login1.Manager")?
            .member("PrepareForSleep")?
            .build();
        self.sleep_events =
            Some(zbus::MessageStream::for_match_rule(rule, &connection, Some(16)).await?);
        Ok(())
    }
    async fn demand(&mut self, active: bool) -> Result<()> {
        self.visible = active;
        self.arm();
        Ok(())
    }
    async fn next(&mut self) -> Result<()> {
        tokio::select! {
            r=self.watch.as_mut().unwrap().changed()=>{r?;let s=settings(&self.config.env_file)?;let changed=self.update_settings(s);self.cache();if changed{self.refreshed=None;self.state.stale=true;self.arm();}Ok(())},
            event=self.sleep_events.as_mut().unwrap().next()=>{let message=event.ok_or_else(||Error::new("bus","logind stream closed"))??;let(sleeping,):(bool,)=message.body().deserialize()?;self.sleeping=sleeping;self.arm();Ok(())},
            _=async{if let Some(d)=self.deadline{tokio::time::sleep_until(d).await}else{std::future::pending().await}}=>{let _=self.refresh().await;Ok(())}
        }
    }
    async fn action(&mut self, _id: u64, a: Action) -> Result<Value> {
        match a {
            Action::Refresh => {
                if self.sleeping {
                    return Err(Error::new("sleep", "weather paused during suspend"));
                }
                self.refresh().await?;
            }
            Action::Day(day) => {
                if day > 4 {
                    return Err(Error::new("day", "weather day must be 0..4"));
                }
                self.state.selected_day = day;
            }
            Action::Next => self.state.selected_day = (self.state.selected_day + 1).min(4),
            Action::Previous => self.state.selected_day = self.state.selected_day.saturating_sub(1),
        }
        std::fs::create_dir_all(&self.config.cache_dir)?;
        std::fs::write(
            self.config.cache_dir.join("view_id"),
            format!("{}\n", self.state.selected_day),
        )?;
        Ok(Value::Null)
    }
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    spawn_with_config(ctx, Config::default())
}
pub fn spawn_with_config(ctx: Context, config: Config) -> Handle<State, Action> {
    launch::<Key, _>(
        ctx,
        Native {
            state: State::default(),
            config,
            settings: None,
            watch: None,
            http: reqwest::Client::new(),
            visible: false,
            sleeping: false,
            deadline: None,
            refreshed: None,
            failures: 0,
            sleep_events: None,
            configuration_dirty: false,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dummy_legacy_schema() {
        let s = dummy(
            TemperatureUnit::Celsius,
            NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
        );
        let v = s.legacy_json();
        assert_eq!(v["forecast"].as_array().unwrap().len(), 5);
        assert_eq!(v["forecast"][0]["max"], "0.0");
        assert_eq!(v["current_hex"], "#cdd6f4");
        assert!(
            import(&v, TemperatureUnit::Celsius, Local::now().date_naive())
                .unwrap()
                .dummy
        );
    }
    #[test]
    fn units_and_icons() {
        assert_eq!(style("01d"), ("", "#f9e2af"));
        assert_eq!(style("13n"), ("", "#cdd6f4"));
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn native(config: Config) -> Native {
        Native {
            state: State::default(),
            config,
            settings: Some(Settings {
                key: "private-key".into(),
                city: "123".into(),
                units: "metric".into(),
            }),
            watch: None,
            http: reqwest::Client::new(),
            visible: false,
            sleeping: false,
            deadline: None,
            refreshed: None,
            failures: 0,
            sleep_events: None,
            configuration_dirty: false,
        }
    }
    #[tokio::test]
    async fn http_normalization_cache_and_demand_deadline() {
        let dir = crate::support::TestDir::new("weather");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let today = Local::now().date_naive();
        let timestamp = today.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp();
        let forecast = json!({"cod":"200","list":[{"dt":timestamp,"dt_txt":format!("{today} 12:00:00"),"main":{"temp":20.2,"temp_min":19.1,"temp_max":22.5,"feels_like":21.1,"humidity":55},"wind":{"speed":3.7},"pop":0.45,"weather":[{"icon":"10d","description":"light rain"}]}]});
        let current = json!({"main":{"temp":21.3},"weather":[{"icon":"01d"}]});
        let server = tokio::spawn(async move {
            for value in [forecast, current] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = [0; 8192];
                let count = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..count]);
                assert!(request.contains("APPID=private-key"));
                let body = value.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let mut native = native(Config {
            env_file: dir.0.join(".env"),
            cache_dir: dir.0.clone(),
            api_base: format!("http://{address}"),
        });
        // Old-location/unit slots must not contaminate a successful replacement.
        atomic(
            &dir.0.join("daily_weather_cache.json"),
            &json!([{
                "dt": timestamp - 10800, "dt_txt": format!("{today} 09:00:00"),
                "main": {"temp":99,"temp_min":99,"temp_max":99}
            }]),
        )
        .unwrap();
        native.configuration_dirty = true;
        native.refresh().await.unwrap();
        assert!(native.deadline.is_none());
        assert_eq!(native.state.days[0].high, 22.5);
        assert_eq!(native.state.days[0].precipitation_percent, 45.);
        assert_eq!(native.state.days[0].description, "Light Rain");
        assert_eq!(native.state.current_temperature, Some(21.3));
        assert_eq!(
            read_json(&dir.0.join("daily_weather_cache.json"))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        native.demand(true).await.unwrap();
        assert!(native.deadline.unwrap() > tokio::time::Instant::now() + Duration::from_secs(898));
        native.demand(false).await.unwrap();
        assert!(native.deadline.is_none());
        let cache = read_json(&dir.0.join("weather.json")).unwrap();
        assert_eq!(cache["current_temp"], "21.3");
        assert_eq!(cache["forecast"][0]["wind"], "4");
        assert!(!cache.to_string().contains("private-key"));
        server.await.unwrap();
        native.visible = true;
        native.config.api_base = "http://127.0.0.1:1".into();
        assert!(native.refresh().await.is_err());
        assert!(native.state.stale);
        assert_eq!(native.state.current_temperature, Some(21.3));
        assert_eq!(read_json(&dir.0.join("weather.json")).unwrap(), cache);
    }
}

#[cfg(test)]
mod adversarial_review {
    use super::*;
    #[tokio::test]
    async fn review_sys_unit_switch_does_not_relabel_old_temperature() {
        let dir = crate::support::TestDir::new("review-weather-unit");
        let env = dir.0.join(".env");
        std::fs::write(
            &env,
            "OPENWEATHER_KEY=test\nOPENWEATHER_CITY_ID=1\nOPENWEATHER_UNIT=metric\n",
        )
        .unwrap();
        let watch = FileWatch::new(vec![env.clone()]).unwrap();
        let (manager, _server, _calls) = crate::systemd::mock::manager().await;
        let mut old = dummy(TemperatureUnit::Celsius, Local::now().date_naive());
        old.dummy = false;
        old.current_temperature = Some(21.0);
        old.days[0].high = 25.0;
        let mut native = Native {
            state: old,
            config: Config {
                env_file: env.clone(),
                cache_dir: dir.0.clone(),
                api_base: "http://127.0.0.1:1".into(),
            },
            settings: Some(settings(&env).unwrap()),
            watch: Some(watch),
            http: reqwest::Client::new(),
            visible: false,
            sleeping: false,
            deadline: None,
            refreshed: Some(SystemTime::now()),
            failures: 0,
            sleep_events: Some(zbus::MessageStream::from(&manager.connection)),
            configuration_dirty: false,
        };
        std::fs::write(
            &env,
            "OPENWEATHER_KEY=test\nOPENWEATHER_CITY_ID=1\nOPENWEATHER_UNIT=imperial\n",
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(1), native.next())
            .await
            .unwrap()
            .unwrap();
        assert!(
            native.state.unit == TemperatureUnit::Celsius
                || native.state.current_temperature == Some(69.8),
            "unit became Fahrenheit but retained temperature={:?} from Celsius cache",
            native.state.current_temperature
        );
    }
}

#[cfg(test)]
mod cache_identity_tests {
    use super::*;
    #[tokio::test]
    async fn changed_city_and_unit_keep_old_identity_when_replacement_fails() {
        let directory = crate::support::TestDir::new("weather-identity");
        let mut old = dummy(TemperatureUnit::Celsius, Local::now().date_naive());
        old.dummy = false;
        old.current_temperature = Some(21.0);
        old.location = Some("1".into());
        old.settings_generation = 1;
        old.data_generation = 1;
        let mut cache = old.legacy_json();
        cache["_hyprv"] = json!({"unit":TemperatureUnit::Celsius,"city":"1"});
        atomic(&directory.0.join("weather.json"), &cache).unwrap();
        let mut native = Native {
            state: old,
            config: Config {
                env_file: directory.0.join(".env"),
                cache_dir: directory.0.clone(),
                api_base: "http://127.0.0.1:1".into(),
            },
            settings: Some(Settings {
                key: "key".into(),
                city: "1".into(),
                units: "metric".into(),
            }),
            watch: None,
            http: reqwest::Client::new(),
            visible: false,
            sleeping: false,
            deadline: None,
            refreshed: Some(SystemTime::now()),
            failures: 0,
            sleep_events: None,
            configuration_dirty: false,
        };
        native.update_settings(Settings {
            key: "new-key".into(),
            city: "2".into(),
            units: "imperial".into(),
        });
        native.cache();
        assert_eq!(native.state.unit, TemperatureUnit::Celsius);
        assert_eq!(native.state.location.as_deref(), Some("1"));
        assert!(native.state.stale);
        assert!(native.state.data_generation < native.state.settings_generation);
        assert!(native.deadline.is_none());
        assert!(native.refresh().await.is_err());
        assert_eq!(native.state.current_temperature, Some(21.0));
        assert_eq!(native.state.unit, TemperatureUnit::Celsius);
        assert_eq!(native.state.location.as_deref(), Some("1"));
        assert_eq!(read_json(&directory.0.join("weather.json")).unwrap(), cache);
    }
    #[test]
    fn legacy_cache_without_unit_identity_is_not_relabelled() {
        let directory = crate::support::TestDir::new("weather-legacy-unit");
        let value = dummy(TemperatureUnit::Celsius, Local::now().date_naive()).legacy_json();
        atomic(&directory.0.join("weather.json"), &value).unwrap();
        let mut native = Native {
            state: State::default(),
            config: Config {
                env_file: directory.0.join(".env"),
                cache_dir: directory.0.clone(),
                api_base: "http://127.0.0.1:1".into(),
            },
            settings: Some(Settings {
                key: "key".into(),
                city: "2".into(),
                units: "imperial".into(),
            }),
            watch: None,
            http: reqwest::Client::new(),
            visible: false,
            sleeping: false,
            deadline: None,
            refreshed: None,
            failures: 0,
            sleep_events: None,
            configuration_dirty: false,
        };
        native.cache();
        assert_eq!(native.state.current_temperature, None);
        assert!(native.state.stale);
        assert_eq!(native.refreshed, None);
    }
}
