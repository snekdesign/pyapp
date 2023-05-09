use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use once_cell::sync::OnceCell;

static PLATFORM_DIRS: OnceCell<ProjectDirs> = OnceCell::new();

fn platform_dirs() -> &'static ProjectDirs {
    PLATFORM_DIRS
        .get()
        .expect("platform directories are not initialized")
}

pub fn initialize() -> Result<()> {
    let platform_dirs = ProjectDirs::from("", "", "pyapp")
        .with_context(|| "unable to find platform directories")?;
    PLATFORM_DIRS
        .set(platform_dirs)
        .expect("could not set platform directories");

    Ok(())
}

pub fn embedded_distribution() -> &'static [u8] {
    // If this is empty, then the distribution will be downloaded at runtime
    include_bytes!("embed/archive")
}

pub fn distribution_id() -> String {
    env!("PYAPP__DISTRIBUTION_ID").into()
}

pub fn distribution_source() -> String {
    env!("PYAPP_DISTRIBUTION_SOURCE").into()
}

pub fn distribution_compression() -> String {
    env!("PYAPP_DISTRIBUTION_COMPRESSION").into()
}

pub fn distribution_python_path() -> String {
    env!("PYAPP_DISTRIBUTION_PYTHON_PATH").into()
}

pub fn project_name() -> String {
    env!("PYAPP_PROJECT_NAME").into()
}

pub fn project_version() -> String {
    env!("PYAPP_PROJECT_VERSION").into()
}

pub fn exec_module() -> String {
    env!("PYAPP_EXEC_MODULE").into()
}

pub fn exec_code() -> String {
    env!("PYAPP_EXEC_CODE").into()
}

pub fn starship_prompt() -> String {
    env!("PYAPP_STARSHIP_PROMPT").into()
}

pub fn cache_directory() -> PathBuf {
    platform_dirs().cache_dir().to_path_buf()
}

pub fn storage_directory() -> PathBuf {
    platform_dirs().data_local_dir().join(project_name())
}

pub fn installation_directory() -> PathBuf {
    storage_directory()
        .join(distribution_id())
        .join(project_version())
}