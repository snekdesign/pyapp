use std::io::Write;

use anyhow::{bail, Context, Result};
use reqwest::header::{ACCEPT, HeaderMap, HeaderName, HeaderValue, USER_AGENT};

use crate::terminal;

pub fn download(url: &String, writer: impl Write, description: &str) -> Result<()> {
    let normalized_url = url.to_lowercase();
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("rust-reqwest/0.12.12"),
    );
    if normalized_url.starts_with(
        "https://api.github.com/repos/adang1345/pythonwindows/contents/3.",
    ) {
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.raw+json"),
        );
        headers.insert(
            HeaderName::from_static("x-github-api-version"),
            HeaderValue::from_static("2022-11-28"),
        );
    }

    let mut response = reqwest::blocking::Client::new()
        .get(url)
        .headers(headers)
        .send()
        .with_context(|| format!("download failed: {}", url))?;

    let pb = terminal::io_progress_bar(
        format!("Downloading {}", description),
        response.content_length().unwrap_or(0),
    );
    response.copy_to(&mut pb.wrap_write(writer))?;
    pb.finish_and_clear();

    if response.status().is_success() {
        Ok(())
    } else {
        bail!("download failed: {}, {}", response.status(), url)
    }
}
