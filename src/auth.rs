const YANDEX_MUSIC_CLIENT_ID: &str = "23cabbbdc6cd418abb4b39c32c41195d";

pub fn authorize_url() -> String {
    format!(
        "https://oauth.yandex.ru/authorize?response_type=token&client_id={YANDEX_MUSIC_CLIENT_ID}"
    )
}

pub fn extract_oauth_token(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(token) = trimmed.strip_prefix("OAuth ") {
        return non_empty(token);
    }

    find_param(trimmed, "access_token").or_else(|| non_empty(trimmed))
}

pub fn extract_oauth_token_from_redirect(input: &str) -> Option<String> {
    find_param(input.trim(), "access_token")
}

fn find_param(input: &str, name: &str) -> Option<String> {
    for section in input.split(['?', '#']) {
        for pair in section.split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if key == name {
                return non_empty(value);
            }
        }
    }

    None
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}
