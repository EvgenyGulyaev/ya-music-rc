use crate::api::ApiError;

const SIGN_SALT: &str = "XGRlBW9FXlekgbPrRHuSiA";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadInfo {
    pub host: String,
    pub path: String,
    pub s: String,
    pub ts: String,
}

pub fn parse_download_xml(xml: &str) -> Result<DownloadInfo, ApiError> {
    Ok(DownloadInfo {
        host: extract_tag(xml, "host")?,
        path: extract_tag(xml, "path")?,
        s: extract_tag(xml, "s")?,
        ts: extract_tag(xml, "ts")?,
    })
}

pub fn build_signed_mp3_url(host: &str, path: &str, s: &str, ts: &str) -> String {
    let path_without_slash = path.strip_prefix('/').unwrap_or(path);
    let digest = md5::compute(format!("{SIGN_SALT}{path_without_slash}{s}"));

    format!("https://{host}/get-mp3/{digest:x}/{ts}{path}")
}

fn extract_tag(xml: &str, tag: &'static str) -> Result<String, ApiError> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml
        .find(&open)
        .ok_or(ApiError::MissingField(tag))?
        .saturating_add(open.len());
    let end = xml[start..]
        .find(&close)
        .ok_or(ApiError::MissingField(tag))?
        .saturating_add(start);

    Ok(xml[start..end].trim().to_owned())
}
