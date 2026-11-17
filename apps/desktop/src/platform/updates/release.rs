//! GitHub release metadata and archive integrity. Never download through a mutable latest URL.

use std::io::{Read, Write};
use std::time::Duration;

use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::Release;

const REPOSITORY: &str = "https://api.github.com/repos/m43i/better-matrix";

#[derive(Clone, Copy, Debug)]
pub enum Platform {
    Mac,
    Windows,
}

impl Platform {
    fn asset(self) -> &'static str {
        match self {
            Self::Mac => "better-matrix-macos-arm64.zip",
            Self::Windows => "better-matrix-windows-x86_64.zip",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Metadata {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    id: u64,
    name: String,
    digest: Option<String>,
}

pub fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!("Better-Matrix/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())
}

pub fn check(client: &Client, platform: Platform) -> Result<Option<Release>, String> {
    let response = client
        .get(format!("{REPOSITORY}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|error| error.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let metadata: Metadata =
        response.error_for_status().map_err(|error| error.to_string())?.json().map_err(|error| error.to_string())?;
    select(metadata, env!("CARGO_PKG_VERSION"), platform)
}

pub fn select(metadata: Metadata, current: &str, platform: Platform) -> Result<Option<Release>, String> {
    let version = Version::parse(metadata.tag_name.trim_start_matches('v')).map_err(|error| error.to_string())?;
    let current = Version::parse(current).map_err(|error| error.to_string())?;
    if metadata.draft || metadata.prerelease || !version.pre.is_empty() || version.cmp_precedence(&current).is_le() {
        return Ok(None);
    }
    let Some(asset) = metadata.assets.into_iter().find(|asset| asset.name == platform.asset()) else {
        return Ok(None);
    };
    let Some(digest) = asset.digest else { return Ok(None) };
    if !valid_digest(&digest) {
        return Err("The release has no valid SHA-256 digest".into());
    }
    Ok(Some(Release { version: version.to_string(), asset_id: asset.id, digest }))
}

fn valid_digest(digest: &str) -> bool {
    digest.strip_prefix("sha256:").is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn download(client: &Client, release: &Release, file: &mut std::fs::File) -> Result<(), String> {
    let response = client
        .get(format!("{REPOSITORY}/releases/assets/{}", release.asset_id))
        .header("Accept", "application/octet-stream")
        .timeout(Duration::from_secs(15 * 60))
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.to_string())?;
    copy_verified(response, file, &release.digest)
}

pub fn copy_verified(mut source: impl Read, target: &mut impl Write, digest: &str) -> Result<(), String> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = source.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        target.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
    }
    if !valid_digest(digest) || !format!("sha256:{:x}", hash.finalize()).eq_ignore_ascii_case(digest) {
        return Err("The update archive failed its SHA-256 check".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
