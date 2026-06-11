use std::cell::RefCell;
use std::collections::VecDeque;

use ya_player::api::{ApiError, HttpClient, TrackSummary, WaveStation, YandexMusicClient};

#[derive(Default)]
struct FakeHttp {
    calls: RefCell<Vec<(String, String)>>,
    response: String,
}

#[derive(Default)]
struct SequenceHttp {
    calls: RefCell<Vec<(String, String)>>,
    responses: RefCell<VecDeque<String>>,
}

impl SequenceHttp {
    fn with_responses(responses: Vec<&str>) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            responses: RefCell::new(responses.into_iter().map(str::to_owned).collect()),
        }
    }
}

impl FakeHttp {
    fn with_response(response: &str) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            response: response.to_owned(),
        }
    }
}

impl HttpClient for FakeHttp {
    fn get(&self, url: &str, token: &str) -> Result<String, ApiError> {
        self.calls
            .borrow_mut()
            .push((url.to_owned(), token.to_owned()));
        Ok(self.response.clone())
    }
}

impl HttpClient for SequenceHttp {
    fn get(&self, url: &str, token: &str) -> Result<String, ApiError> {
        self.calls
            .borrow_mut()
            .push((url.to_owned(), token.to_owned()));
        self.responses
            .borrow_mut()
            .pop_front()
            .ok_or(ApiError::Http("missing fake response".to_owned()))
    }

    fn post_json(
        &self,
        url: &str,
        token: &str,
        _body: serde_json::Value,
    ) -> Result<String, ApiError> {
        self.calls
            .borrow_mut()
            .push((url.to_owned(), token.to_owned()));
        self.responses
            .borrow_mut()
            .pop_front()
            .ok_or(ApiError::Http("missing fake response".to_owned()))
    }
}

#[test]
fn account_status_parses_user_identity() {
    let http = FakeHttp::with_response(
        r#"{"account":{"uid":42,"login":"demo-user"},"permissions":{"until":"2099-01-01"}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let status = client.account_status().expect("account status");

    assert_eq!(status.uid, 42);
    assert_eq!(status.login, "demo-user");
    assert_eq!(status.display_name, "demo-user");
}

#[test]
fn account_status_parses_result_wrapped_identity() {
    let http = FakeHttp::with_response(
        r#"{"result":{"account":{"uid":42,"login":"demo-user"},"permissions":{"until":"2099-01-01"}}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let status = client.account_status().expect("account status");

    assert_eq!(status.uid, 42);
    assert_eq!(status.login, "demo-user");
    assert_eq!(status.display_name, "demo-user");
}

#[test]
fn account_status_prefers_real_display_name() {
    let http = FakeHttp::with_response(
        r#"{"result":{"account":{"uid":42,"login":"demo-user","firstName":"Demo","lastName":"User"}}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let status = client.account_status().expect("account status");

    assert_eq!(status.login, "demo-user");
    assert_eq!(status.display_name, "Demo User");
}

#[test]
fn liked_tracks_parse_common_library_shape() {
    let http = FakeHttp::with_response(
        r#"{"library":{"tracks":[{"id":"100","albumId":"200","title":"Song","artists":[{"name":"Artist"}]}]}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let tracks = client.liked_tracks(42).expect("liked tracks");

    assert_eq!(
        tracks,
        vec![TrackSummary {
            id: "100".to_owned(),
            album_id: Some("200".to_owned()),
            title: "Song".to_owned(),
            artist: "Artist".to_owned(),
        }]
    );
}

#[test]
fn liked_tracks_parse_result_wrapped_library_shape() {
    let http = FakeHttp::with_response(
        r#"{"result":{"library":{"tracks":[{"id":"100","albumId":"200","title":"Song","artists":[{"name":"Artist"}]}]}}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let tracks = client.liked_tracks(42).expect("liked tracks");

    assert_eq!(tracks[0].id, "100");
    assert_eq!(tracks[0].title, "Song");
}

#[test]
fn liked_tracks_resolve_metadata_when_library_contains_only_track_refs() {
    let http = SequenceHttp::with_responses(vec![
        r#"{"result":{"library":{"tracks":[{"id":"100","albumId":"200"}]}}}"#,
        r#"{"result":[{"id":"100","title":"Resolved Song","artists":[{"name":"Resolved Artist"}],"albums":[{"id":"200"}]}]}"#,
    ]);
    let client = YandexMusicClient::new("token".to_owned(), http);

    let tracks = client.liked_tracks(42).expect("liked tracks");

    assert_eq!(
        tracks,
        vec![TrackSummary {
            id: "100".to_owned(),
            album_id: Some("200".to_owned()),
            title: "Resolved Song".to_owned(),
            artist: "Resolved Artist".to_owned(),
        }]
    );
    let calls = client.http().calls.borrow();
    assert!(calls[0].0.ends_with("/users/42/likes/tracks"));
    assert!(calls[1].0.ends_with("/tracks?trackIds=100:200"));
}

#[test]
fn wave_stations_parse_rotor_shape() {
    let http = FakeHttp::with_response(
        r#"{"stations":[{"station":{"id":{"type":"user","tag":"onyourwave"},"name":"Моя волна"}}]}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let stations = client.wave_stations().expect("wave stations");

    assert_eq!(
        stations,
        vec![WaveStation {
            station_type: "user".to_owned(),
            tag: "onyourwave".to_owned(),
            name: "Моя волна".to_owned(),
        }]
    );
}

#[test]
fn wave_stations_parse_result_wrapped_rotor_shape() {
    let http = FakeHttp::with_response(
        r#"{"result":{"stations":[{"station":{"id":{"type":"user","tag":"onyourwave"},"name":"Моя волна"}}]}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let stations = client.wave_stations().expect("wave stations");

    assert_eq!(stations[0].tag, "onyourwave");
}

#[test]
fn wave_stations_parse_nested_dashboard_shape() {
    let http = FakeHttp::with_response(
        r#"{"result":{"dashboard":{"stations":[{"station":{"id":{"type":"user","tag":"onyourwave"},"name":"Моя волна"}}]}}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let stations = client.wave_stations().expect("wave stations");

    assert_eq!(stations[0].tag, "onyourwave");
}

#[test]
fn wave_stations_fallback_to_default_wave_when_list_is_absent() {
    let http = FakeHttp::with_response(r#"{"result":{"dashboard":{}}}"#);
    let client = YandexMusicClient::new("token".to_owned(), http);

    let stations = client.wave_stations().expect("wave stations");

    assert_eq!(
        stations,
        vec![WaveStation {
            station_type: "user".to_owned(),
            tag: "onyourwave".to_owned(),
            name: "Моя волна".to_owned(),
        }]
    );
}

#[test]
fn station_tracks_parse_rotor_sequence_shape() {
    let http = FakeHttp::with_response(
        r#"{"sequence":[{"type":"track","track":{"id":"300","albumId":"400","title":"Wave Song","artists":[{"name":"Wave Artist"}]},"liked":false}]}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);
    let station = WaveStation {
        station_type: "user".to_owned(),
        tag: "onyourwave".to_owned(),
        name: "Моя волна".to_owned(),
    };

    let tracks = client.station_tracks(&station).expect("station tracks");

    assert_eq!(
        tracks,
        vec![TrackSummary {
            id: "300".to_owned(),
            album_id: Some("400".to_owned()),
            title: "Wave Song".to_owned(),
            artist: "Wave Artist".to_owned(),
        }]
    );
    assert!(
        client.http().calls.borrow()[0]
            .0
            .ends_with("/rotor/station/user:onyourwave/tracks?settings2=true")
    );
}

#[test]
fn station_tracks_parse_result_wrapped_rotor_sequence_shape() {
    let http = FakeHttp::with_response(
        r#"{"result":{"sequence":[{"type":"track","track":{"id":"300","albumId":"400","title":"Wave Song","artists":[{"name":"Wave Artist"}]}}]}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);
    let station = WaveStation {
        station_type: "user".to_owned(),
        tag: "onyourwave".to_owned(),
        name: "Моя волна".to_owned(),
    };

    let tracks = client.station_tracks(&station).expect("station tracks");

    assert_eq!(tracks[0].id, "300");
    assert_eq!(tracks[0].title, "Wave Song");
}

#[test]
fn station_session_tracks_parse_session_sequence_shape() {
    let http = SequenceHttp::with_responses(vec![
        r#"{"result":{"batchId":"batch-1","pumpkin":false,"sequence":[{"type":"track","track":{"id":"500","title":"Session Song","artists":[{"name":"Session Artist"}],"albums":[{"id":"600"}]},"liked":false,"trackParameters":{}}],"terminated":false}}"#,
    ]);
    let client = YandexMusicClient::new("token".to_owned(), http);
    let station = WaveStation {
        station_type: "user".to_owned(),
        tag: "onyourwave".to_owned(),
        name: "Моя волна".to_owned(),
    };

    let tracks = client
        .station_session_tracks(&station)
        .expect("station session tracks");

    assert_eq!(
        tracks,
        vec![TrackSummary {
            id: "500".to_owned(),
            album_id: Some("600".to_owned()),
            title: "Session Song".to_owned(),
            artist: "Session Artist".to_owned(),
        }]
    );
    assert!(
        client.http().calls.borrow()[0]
            .0
            .ends_with("/rotor/session/new")
    );
}

#[test]
fn search_tracks_parse_track_results() {
    let http = FakeHttp::with_response(
        r#"{"result":{"tracks":{"results":[{"id":"700","title":"Found Song","artists":[{"name":"Found Artist"}],"albums":[{"id":"800"}]}]}}}"#,
    );
    let client = YandexMusicClient::new("token".to_owned(), http);

    let tracks = client.search_tracks("found song").expect("search tracks");

    assert_eq!(
        tracks,
        vec![TrackSummary {
            id: "700".to_owned(),
            album_id: Some("800".to_owned()),
            title: "Found Song".to_owned(),
            artist: "Found Artist".to_owned(),
        }]
    );
    let calls = client.http().calls.borrow();
    assert!(
        calls[0]
            .0
            .starts_with("https://api.music.yandex.net/search?")
    );
    assert!(calls[0].0.contains("type=track"));
    assert!(calls[0].0.contains("text=found+song"));
}

#[test]
fn search_tracks_skip_empty_query() {
    let http = FakeHttp::with_response(r#"{}"#);
    let client = YandexMusicClient::new("token".to_owned(), http);

    let tracks = client.search_tracks("  ").expect("search tracks");

    assert!(tracks.is_empty());
    assert!(client.http().calls.borrow().is_empty());
}

#[test]
fn requests_use_expected_endpoints_and_token() {
    let http = FakeHttp::with_response(r#"{"account":{"uid":1,"login":"x"}}"#);
    let client = YandexMusicClient::new("secret".to_owned(), http);

    let _ = client.account_status().expect("account status");

    let calls = client.http().calls.borrow();
    assert_eq!(calls[0].1, "secret");
    assert!(calls[0].0.ends_with("/account/status"));
}
