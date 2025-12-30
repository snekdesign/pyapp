use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use http::Extensions;
use rand::{rng, prelude::IndexedRandom};
use rattler::install::Installer;
use rattler_conda_types::{Platform, RepoDataRecord};
use rattler_lock::{
    CondaPackageData, DEFAULT_ENVIRONMENT_NAME, LockFile, LockedPackageRef, UrlOrPath,
};
use reqwest::{Request, Response};
use reqwest_middleware::{Error, Middleware, Next};
use reqwest_retry::{
    Retryable,
    RetryableStrategy,
    RetryTransientMiddleware,
    policies::ExponentialBackoff,
};
use url::Url;

use crate::terminal;

struct RandomMirrorMiddleware {
    mirrors: Vec<Url>,
}

#[async_trait]
impl Middleware for RandomMirrorMiddleware {
    async fn handle(
        &self,
        mut req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        let url_str = req.url().as_str();
        if let Some(url_rest) = url_str.strip_prefix("https://conda.anaconda.org/") {
            let url_rest = url_rest.trim_start_matches('/');
            if let Some(selected_mirror) = self.mirrors.choose(&mut rng()) {
                let selected_url = selected_mirror
                    .join(url_rest)
                    .map_err(|e| Error::Middleware(e.into()))?;
                *req.url_mut() = selected_url;
            }
        }
        next.run(req, extensions).await
    }
}

struct Retry4xx;

impl RetryableStrategy for Retry4xx {
    fn handle(&self, res: &Result<Response, Error>) -> Option<Retryable> {
        match res {
            Ok(success) if success.status().is_client_error() => Some(Retryable::Transient),
            _ => uv_client::UvRetryableStrategy.handle(res),
        }
    }
}

pub fn unpack(
    format: String,
    archive: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<()> {
    let wait_message = format!("Unpacking distribution ({})", format);
    match format.as_ref() {
        "pixi.lock" => unpack_pixi_install_to_prefix(archive, destination)?,
        "tar|bzip2" => unpack_tar_bzip2(archive, destination, wait_message)?,
        "tar|gzip" => unpack_tar_gzip(archive, destination, wait_message)?,
        "tar|zstd" => unpack_tar_zstd(archive, destination, wait_message)?,
        "zip" => unpack_zip(archive, destination, wait_message)?,
        _ => bail!("unsupported distribution format: {}", format),
    }

    Ok(())
}

fn unpack_pixi_install_to_prefix(
    path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<()> {
    let timeout = 5 * 60;
    let download_client = reqwest_middleware::ClientBuilder::new(
            reqwest::Client::builder()
                .no_gzip()
                .pool_max_idle_per_host(20)
                .user_agent("pixi-install-to-prefix/0.1.2")
                .timeout(std::time::Duration::from_secs(timeout))
                .build()
                .map_err(|e| anyhow!("could not create download client: {}", e))?,
        )
        .with(RetryTransientMiddleware::new_with_policy_and_strategy(
            ExponentialBackoff::builder().build_with_max_retries(3),
            Retry4xx,
        ))
        .with(RandomMirrorMiddleware {
            mirrors: vec![
                Url::parse("https://conda.anaconda.org/")?,
                Url::parse("https://mirror.nju.edu.cn/anaconda/cloud/")?,
                Url::parse("https://mirror.nyist.edu.cn/anaconda/cloud/")?,
                Url::parse("https://mirrors.cqupt.edu.cn/anaconda/cloud/")?,
                Url::parse("https://mirrors.hit.edu.cn/anaconda/cloud/")?,
                Url::parse("https://mirrors.lzu.edu.cn/anaconda/cloud/")?,
                Url::parse("https://mirrors.pku.edu.cn/anaconda/cloud/")?,
                Url::parse("https://mirrors.sustech.edu.cn/anaconda/cloud/")?,
                Url::parse("https://mirrors.tuna.tsinghua.edu.cn/anaconda/cloud/")?,
                Url::parse("https://prefix.dev/")?,
            ],
        })
        .build();

    let lockfile = LockFile::from_path(path.as_ref()).map_err(|e| {
        anyhow!(
            "could not read lockfile at {}: {}",
            path.as_ref().display(),
            e
        )
    })?;

    let environment = lockfile
        .default_environment()
        .ok_or(anyhow!(
            "Environment {} not found in lockfile",
            DEFAULT_ENVIRONMENT_NAME
        ))?;
    let packages = environment.packages(Platform::current()).ok_or(anyhow!(
        "environment {} does not contain platform {}",
        DEFAULT_ENVIRONMENT_NAME,
        Platform::current()
    ))?;

    let packages = packages
        .map(|p| match p {
            LockedPackageRef::Conda(p) => match p {
                CondaPackageData::Binary(p) => Ok(RepoDataRecord {
                    package_record: p.package_record.clone(),
                    file_name: p.file_name.clone(),
                    url: match p.location.clone() {
                        UrlOrPath::Url(url) => url,
                        UrlOrPath::Path(_) => {
                            Err(anyhow!("Path package {} is not supported", p.location))?
                        }
                    },
                    channel: p.channel.clone().map(|c| c.to_string()),
                }),
                CondaPackageData::Source(p) => {
                    Err(anyhow!("Source package {} is not supported", p.location))
                }
            },
            LockedPackageRef::Pypi(_, _) => {
                Err(anyhow!("Pypi package {} is not supported", p.location()))
            }
        })
        .collect::<Result<Vec<_>>>()?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(
            Installer::new()
                .with_download_client(download_client)
                .with_target_platform(Platform::current())
                .with_execute_link_scripts(true)
                .with_reporter(rattler::install::IndicatifReporter::builder().finish())
                .install(&destination, packages)
        )?;

    Ok(())
}

fn unpack_tar_bzip2(
    path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    wait_message: String,
) -> Result<()> {
    let bz = bzip2::read::BzDecoder::new(File::open(path)?);
    let mut archive = tar::Archive::new(bz);

    let spinner = terminal::spinner(wait_message);
    let result = archive.unpack(destination);
    spinner.finish_and_clear();
    result?;

    Ok(())
}

pub fn unpack_tar_gzip(
    path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    wait_message: String,
) -> Result<()> {
    let gz = flate2::read::GzDecoder::new(File::open(path)?);
    let mut archive = tar::Archive::new(gz);

    let spinner = terminal::spinner(wait_message);
    let result = archive.unpack(destination);
    spinner.finish_and_clear();
    result?;

    Ok(())
}

fn unpack_tar_zstd(
    path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    wait_message: String,
) -> Result<()> {
    let zst = zstd::stream::read::Decoder::new(File::open(path)?)?;
    let mut archive = tar::Archive::new(zst);

    let spinner = terminal::spinner(wait_message);
    let result = archive.unpack(destination);
    spinner.finish_and_clear();
    result?;

    Ok(())
}

pub fn unpack_zip(
    path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    wait_message: String,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;

    let spinner = terminal::spinner(wait_message);
    let result = archive.extract(destination);
    spinner.finish_and_clear();
    result?;

    Ok(())
}
