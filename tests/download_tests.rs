use std::cell::RefCell;

use ya_player::api::{ApiError, HttpClient, YandexMusicClient};
use ya_player::download::{build_signed_mp3_url, parse_download_xml};

struct FakeHttp {
    responses: RefCell<Vec<String>>,
    calls: RefCell<Vec<String>>,
}

impl FakeHttp {
    fn new(responses: Vec<&str>) -> Self {
        Self {
            responses: RefCell::new(responses.into_iter().map(str::to_owned).collect()),
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl HttpClient for FakeHttp {
    fn get(&self, url: &str, _token: &str) -> Result<String, ApiError> {
        self.calls.borrow_mut().push(url.to_owned());
        Ok(self.responses.borrow_mut().remove(0))
    }
}

#[test]
fn parse_download_xml_extracts_signing_parts() {
    let info = parse_download_xml(
        r#"<download-info><host>storage.example</host><path>/music/track.mp3</path><s>abc123</s><ts>999</ts></download-info>"#,
    )
    .expect("download xml");

    assert_eq!(info.host, "storage.example");
    assert_eq!(info.path, "/music/track.mp3");
    assert_eq!(info.s, "abc123");
    assert_eq!(info.ts, "999");
}

#[test]
fn signed_mp3_url_uses_yandex_signing_algorithm() {
    let url = build_signed_mp3_url("storage.example", "/music/track.mp3", "abc123", "999");

    assert_eq!(
        url,
        "https://storage.example/get-mp3/3153457edd3e2e632420ee5e22a88cb5/999/music/track.mp3"
    );
}

#[test]
fn client_resolves_track_playback_url_from_download_info() {
    let http = FakeHttp::new(vec![
        r#"[{"codec":"aac","bitrateInKbps":192,"downloadInfoUrl":"https://ignore/aac"},{"codec":"mp3","bitrateInKbps":320,"downloadInfoUrl":"https://download/info"}]"#,
        r#"<download-info><host>storage.example</host><path>/music/track.mp3</path><s>abc123</s><ts>999</ts></download-info>"#,
    ]);
    let client = YandexMusicClient::new("token".to_owned(), http);

    let url = client
        .track_playback_url("100", Some("200"))
        .expect("playback url");

    assert_eq!(
        url.as_str(),
        "https://storage.example/get-mp3/3153457edd3e2e632420ee5e22a88cb5/999/music/track.mp3"
    );
    let calls = client.http().calls.borrow();
    assert!(calls[0].ends_with("/tracks/100:200/download-info"));
    assert_eq!(calls[1], "https://download/info");
}
