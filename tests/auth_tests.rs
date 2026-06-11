use ya_player::auth::{authorize_url, extract_oauth_token, extract_oauth_token_from_redirect};

#[test]
fn authorize_url_uses_yandex_implicit_flow() {
    let url = authorize_url();

    assert!(url.starts_with("https://oauth.yandex.ru/authorize?"));
    assert!(url.contains("response_type=token"));
    assert!(url.contains("client_id=23cabbbdc6cd418abb4b39c32c41195d"));
}

#[test]
fn token_input_accepts_raw_token() {
    assert_eq!(
        extract_oauth_token(" AQAAA-token ").as_deref(),
        Some("AQAAA-token")
    );
}

#[test]
fn token_input_accepts_oauth_prefix() {
    assert_eq!(
        extract_oauth_token("OAuth AQAAA-token").as_deref(),
        Some("AQAAA-token")
    );
}

#[test]
fn token_input_accepts_redirect_fragment() {
    assert_eq!(
        extract_oauth_token(
            "https://music.yandex.ru/#access_token=AQAAA-token&token_type=bearer&expires_in=31536000"
        )
        .as_deref(),
        Some("AQAAA-token")
    );
}

#[test]
fn token_input_accepts_yandex_music_redirect_token_shape() {
    assert_eq!(
        extract_oauth_token(
            "https://music.yandex.ru/#access_token=y0__wExampleToken&token_type=bearer&expires_in=31535700&cid=vb24w2mfezxbe3mctje5vjp6vw"
        )
        .as_deref(),
        Some("y0__wExampleToken")
    );
}

#[test]
fn token_input_accepts_redirect_query() {
    assert_eq!(
        extract_oauth_token(
            "https://oauth.yandex.ru/verification_code?access_token=AQAAA-token&token_type=bearer"
        )
        .as_deref(),
        Some("AQAAA-token")
    );
}

#[test]
fn redirect_extractor_ignores_pages_without_access_token() {
    assert_eq!(
        extract_oauth_token_from_redirect("https://music.yandex.ru/home"),
        None
    );
}

#[test]
fn redirect_extractor_accepts_access_token_fragment() {
    assert_eq!(
        extract_oauth_token_from_redirect(
            "https://music.yandex.ru/#access_token=y0__wExampleToken&token_type=bearer"
        )
        .as_deref(),
        Some("y0__wExampleToken")
    );
}
