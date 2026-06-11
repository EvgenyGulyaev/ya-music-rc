use std::error::Error;
use std::fmt;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;

use crate::download::{build_signed_mp3_url, parse_download_xml};

const API_BASE: &str = "https://api.music.yandex.net";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    pub uid: u64,
    pub login: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSummary {
    pub id: String,
    pub album_id: Option<String>,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveStation {
    pub station_type: String,
    pub tag: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackUrl(String);

impl PlaybackUrl {
    pub fn new(url: String) -> Self {
        Self(url)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug)]
pub enum ApiError {
    Http(String),
    Parse(String),
    MissingField(&'static str),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Http(message) => write!(f, "HTTP error: {message}"),
            ApiError::Parse(message) => write!(f, "parse error: {message}"),
            ApiError::MissingField(field) => write!(f, "missing field: {field}"),
        }
    }
}

impl Error for ApiError {}

pub trait HttpClient {
    fn get(&self, url: &str, token: &str) -> Result<String, ApiError>;

    fn post_json(&self, _url: &str, _token: &str, _body: Value) -> Result<String, ApiError> {
        Err(ApiError::Http("POST is not implemented".to_owned()))
    }
}

pub struct ReqwestHttpClient {
    client: Client,
}

impl Default for ReqwestHttpClient {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("reqwest client with timeout"),
        }
    }
}

impl HttpClient for ReqwestHttpClient {
    fn get(&self, url: &str, token: &str) -> Result<String, ApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("OAuth {token}"))
                .map_err(|err| ApiError::Http(err.to_string()))?,
        );
        headers.insert(
            "X-Yandex-Music-Client",
            HeaderValue::from_static("YaPlayerRust/0.1"),
        );

        self.client
            .get(url)
            .headers(headers)
            .send()
            .map_err(|err| ApiError::Http(err.to_string()))?
            .error_for_status()
            .map_err(|err| ApiError::Http(err.to_string()))?
            .text()
            .map_err(|err| ApiError::Http(err.to_string()))
    }

    fn post_json(&self, url: &str, token: &str, body: Value) -> Result<String, ApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("OAuth {token}"))
                .map_err(|err| ApiError::Http(err.to_string()))?,
        );
        headers.insert(
            "X-Yandex-Music-Client",
            HeaderValue::from_static("YaPlayerRust/0.1"),
        );

        self.client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .map_err(|err| ApiError::Http(err.to_string()))?
            .error_for_status()
            .map_err(|err| ApiError::Http(err.to_string()))?
            .text()
            .map_err(|err| ApiError::Http(err.to_string()))
    }
}

pub struct YandexMusicClient<H> {
    token: String,
    http: H,
}

impl<H> YandexMusicClient<H> {
    pub fn new(token: String, http: H) -> Self {
        Self { token, http }
    }

    pub fn http(&self) -> &H {
        &self.http
    }
}

impl<H: HttpClient> YandexMusicClient<H> {
    pub fn account_status(&self) -> Result<AccountStatus, ApiError> {
        let value = self.get_json("/account/status")?;
        let payload = payload(&value);
        let account = payload
            .get("account")
            .ok_or(ApiError::MissingField("account"))?;
        let login = string_field(account, "login")
            .ok_or(ApiError::MissingField("account.login"))?
            .to_owned();
        Ok(AccountStatus {
            uid: account
                .get("uid")
                .and_then(Value::as_u64)
                .ok_or(ApiError::MissingField("account.uid"))?,
            display_name: account_display_name(account).unwrap_or_else(|| login.clone()),
            login,
        })
    }

    pub fn liked_tracks(&self, uid: u64) -> Result<Vec<TrackSummary>, ApiError> {
        let value = self.get_json(&format!("/users/{uid}/likes/tracks"))?;
        let tracks = payload(&value)
            .pointer("/library/tracks")
            .and_then(Value::as_array)
            .ok_or(ApiError::MissingField("library.tracks"))?;

        if tracks.iter().all(has_track_metadata) {
            return Ok(tracks.iter().map(parse_track_summary).collect());
        }

        self.track_summaries(track_ids_param(tracks)?)
    }

    pub fn wave_stations(&self) -> Result<Vec<WaveStation>, ApiError> {
        let value = self.get_json("/rotor/stations/list")?;
        let Some(stations) = station_entries(payload(&value)) else {
            return Ok(default_wave_stations());
        };

        Ok(stations.iter().filter_map(parse_wave_station).collect())
    }

    pub fn station_tracks(&self, station: &WaveStation) -> Result<Vec<TrackSummary>, ApiError> {
        let station_id = format!("{}:{}", station.station_type, station.tag);
        let value = self.get_json(&format!(
            "/rotor/station/{station_id}/tracks?settings2=true"
        ))?;
        let sequence = payload(&value)
            .get("sequence")
            .and_then(Value::as_array)
            .ok_or(ApiError::MissingField("sequence"))?;

        Ok(sequence
            .iter()
            .filter_map(|entry| entry.get("track"))
            .map(parse_track_summary)
            .collect())
    }

    pub fn station_session_tracks(
        &self,
        station: &WaveStation,
    ) -> Result<Vec<TrackSummary>, ApiError> {
        let station_id = format!("{}:{}", station.station_type, station.tag);
        let value = self.post_json(
            "/rotor/session/new",
            serde_json::json!({
                "seeds": [station_id],
                "queue": [],
                "includeTracksInResponse": true,
                "includeWaveModel": true,
                "interactive": false,
            }),
        )?;
        let sequence = payload(&value)
            .get("sequence")
            .and_then(Value::as_array)
            .ok_or(ApiError::MissingField("sequence"))?;

        Ok(sequence
            .iter()
            .filter_map(|entry| entry.get("track"))
            .map(parse_track_summary)
            .collect())
    }

    pub fn track_summaries(&self, track_ids: String) -> Result<Vec<TrackSummary>, ApiError> {
        if track_ids.is_empty() {
            return Ok(Vec::new());
        }

        let value = self.get_json(&format!("/tracks?trackIds={track_ids}"))?;
        let tracks = payload(&value)
            .as_array()
            .ok_or(ApiError::MissingField("tracks[]"))?;

        Ok(tracks.iter().map(parse_track_summary).collect())
    }

    pub fn track_playback_url(
        &self,
        track_id: &str,
        album_id: Option<&str>,
    ) -> Result<PlaybackUrl, ApiError> {
        let track_ref = match album_id {
            Some(album_id) if !album_id.is_empty() => format!("{track_id}:{album_id}"),
            _ => track_id.to_owned(),
        };
        let value = self.get_json(&format!("/tracks/{track_ref}/download-info"))?;
        let entries = payload(&value)
            .as_array()
            .ok_or(ApiError::MissingField("download-info[]"))?;
        let best_mp3 = entries
            .iter()
            .filter(|entry| string_field(entry, "codec") == Some("mp3"))
            .max_by_key(|entry| {
                entry
                    .get("bitrateInKbps")
                    .and_then(Value::as_u64)
                    .unwrap_or_default()
            })
            .or_else(|| entries.first())
            .ok_or(ApiError::MissingField("downloadInfoUrl"))?;
        let download_info_url = string_field(best_mp3, "downloadInfoUrl")
            .ok_or(ApiError::MissingField("downloadInfoUrl"))?;
        let xml = self.http.get(download_info_url, &self.token)?;
        let info = parse_download_xml(&xml)?;

        Ok(PlaybackUrl::new(build_signed_mp3_url(
            &info.host, &info.path, &info.s, &info.ts,
        )))
    }

    fn get_json(&self, path: &str) -> Result<Value, ApiError> {
        let url = format!("{API_BASE}{path}");
        let body = self.http.get(&url, &self.token)?;
        serde_json::from_str(&body).map_err(|err| ApiError::Parse(err.to_string()))
    }

    fn post_json(&self, path: &str, body: Value) -> Result<Value, ApiError> {
        let url = format!("{API_BASE}{path}");
        let body = self.http.post_json(&url, &self.token, body)?;
        serde_json::from_str(&body).map_err(|err| ApiError::Parse(err.to_string()))
    }
}

fn parse_track_summary(value: &Value) -> TrackSummary {
    let artist = value
        .get("artists")
        .and_then(Value::as_array)
        .and_then(|artists| artists.first())
        .and_then(|artist| string_field(artist, "name"))
        .unwrap_or("Unknown artist")
        .to_owned();

    TrackSummary {
        id: value_to_string(value.get("id")).unwrap_or_else(|| "unknown".to_owned()),
        album_id: value_to_string(value.get("albumId")).or_else(|| {
            value
                .get("albums")
                .and_then(Value::as_array)
                .and_then(|albums| albums.first())
                .and_then(|album| value_to_string(album.get("id")))
        }),
        title: string_field(value, "title")
            .unwrap_or("Untitled")
            .to_owned(),
        artist,
    }
}

fn account_display_name(account: &Value) -> Option<String> {
    if let Some(display_name) = string_field(account, "displayName").and_then(non_empty_string) {
        return Some(display_name);
    }

    let first_name = string_field(account, "firstName").and_then(non_empty_string);
    let last_name = string_field(account, "lastName")
        .or_else(|| string_field(account, "secondName"))
        .and_then(non_empty_string);

    match (first_name, last_name) {
        (Some(first), Some(last)) => Some(format!("{first} {last}")),
        (Some(first), None) => Some(first),
        (None, Some(last)) => Some(last),
        (None, None) => None,
    }
}

fn has_track_metadata(value: &Value) -> bool {
    string_field(value, "title").is_some()
        && value
            .get("artists")
            .and_then(Value::as_array)
            .is_some_and(|artists| !artists.is_empty())
}

fn track_ids_param(tracks: &[Value]) -> Result<String, ApiError> {
    let mut ids = Vec::new();

    for track in tracks {
        let id = value_to_string(track.get("id")).ok_or(ApiError::MissingField("track.id"))?;
        match value_to_string(track.get("albumId")) {
            Some(album_id) => ids.push(format!("{id}:{album_id}")),
            None => ids.push(id),
        }
    }

    Ok(ids.join(","))
}

fn payload(value: &Value) -> &Value {
    value.get("result").unwrap_or(value)
}

fn parse_wave_station(value: &Value) -> Option<WaveStation> {
    let station = value.get("station").unwrap_or(value);
    let id = station.get("id")?;

    Some(WaveStation {
        station_type: string_field(id, "type").unwrap_or("unknown").to_owned(),
        tag: string_field(id, "tag").unwrap_or("unknown").to_owned(),
        name: string_field(station, "name")
            .unwrap_or("Unnamed wave")
            .to_owned(),
    })
}

fn default_wave_stations() -> Vec<WaveStation> {
    vec![WaveStation {
        station_type: "user".to_owned(),
        tag: "onyourwave".to_owned(),
        name: "Моя волна".to_owned(),
    }]
}

fn station_entries(value: &Value) -> Option<&Vec<Value>> {
    if let Some(stations) = value.get("stations").and_then(Value::as_array) {
        return Some(stations);
    }

    match value {
        Value::Object(map) => map.values().find_map(station_entries),
        Value::Array(items) => items.iter().find_map(station_entries),
        _ => None,
    }
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn value_to_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}
