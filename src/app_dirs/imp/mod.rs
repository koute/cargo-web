use super::common::{AppDataType, AppDirsError, AppInfo};
use std::fs;
use std::path::PathBuf;
use super::utils;

#[cfg(target_os="macos")]
mod platform {
    mod macos;
    pub use self::macos::*;
}
#[cfg(all(unix, not(target_os="macos")))]
mod platform {
    mod unix;
    pub use self::unix::*;
}
#[cfg(windows)]
mod platform {
    mod windows;
    pub use self::windows::*;
}
#[cfg(not(any(windows, unix, target_os="macos",)))]
mod platform {
    mod unknown;
    pub use self::unknown::*;
}

/// Creates (if necessary) and returns path to **app-specific** data
/// **subdirectory** for provided data type and subdirectory path.
///
/// The `path` parameter should be a valid relative path separated by
/// **forward slashes** (`/`).
///
/// If the directory structure does not exist, this function will recursively
/// create the full hierarchy. Therefore, a result of `Ok` guarantees that the
/// returned path exists.
pub fn app_dir(t: AppDataType, app: &AppInfo, path: &str) -> Result<PathBuf, AppDirsError> {
    let path = try!(get_app_dir(t, app, &path));
    match fs::create_dir_all(&path) {
        Ok(..) => Ok(path),
        Err(e) => Err(e.into()),
    }
}

/// Returns (but **does not create**) path to **app-specific** data
/// **subdirectory** for provided data type and subdirectory path.
///
/// The `path` parameter should be a valid relative path separated by
/// **forward slashes** (`/`).
///
/// A result of `Ok` means that we determined where the data SHOULD go, but
/// it DOES NOT guarantee that the directory actually exists. (See
/// [`app_dir`](fn.app_dir.html).)
pub fn get_app_dir(t: AppDataType, app: &AppInfo, path: &str) -> Result<PathBuf, AppDirsError> {
    if app.author.len() == 0 || app.name.len() == 0 {
        return Err(AppDirsError::InvalidAppInfo);
    }
    app_root(t, app).map(|mut root| {
        for component in path.split("/").filter(|s| s.len() > 0) {
            root.push(utils::sanitized(component));
        }
        root
    })
}

/// Creates (if necessary) and returns path to **app-specific** data
/// directory for provided data type.
///
/// If the directory structure does not exist, this function will recursively
/// create the full hierarchy. Therefore, a result of `Ok` guarantees that the
/// returned path exists.
pub fn app_root(t: AppDataType, app: &AppInfo) -> Result<PathBuf, AppDirsError> {
    let path = try!(get_app_root(t, app));
    match fs::create_dir_all(&path) {
        Ok(..) => Ok(path),
        Err(e) => Err(e.into()),
    }
}

/// Returns (but **does not create**) path to **app-specific** data directory
/// for provided data type.
///
/// A result of `Ok` means that we determined where the data SHOULD go, but
/// it DOES NOT guarantee that the directory actually exists. (See
/// [`app_root`](fn.app_root.html).)
pub fn get_app_root(t: AppDataType, app: &AppInfo) -> Result<PathBuf, AppDirsError> {
    if app.author.len() == 0 || app.name.len() == 0 {
        return Err(AppDirsError::InvalidAppInfo);
    }
    data_root(t).map(|mut root| {
        if platform::USE_AUTHOR {
            root.push(utils::sanitized(app.author));
        }
        root.push(utils::sanitized(app.name));
        root
    })
}

/// Creates (if necessary) and returns path to **top-level** data directory
/// for provided data type.
///
/// If the directory structure does not exist, this function will recursively
/// create the full hierarchy. Therefore, a result of `Ok` guarantees that the
/// returned path exists.
pub fn data_root(t: AppDataType) -> Result<PathBuf, AppDirsError> {
    let path = try!(platform::get_app_dir(t));
    match fs::create_dir_all(&path) {
        Ok(..) => Ok(path),
        Err(e) => Err(e.into()),
    }
}

/// Returns (but **does not create**) path to **top-level** data directory for
/// provided data type.
///
/// A result of `Ok` means that we determined where the data SHOULD go, but
/// it DOES NOT guarantee that the directory actually exists. (See
/// [`data_root`](fn.data_root.html).)
pub fn get_data_root(t: AppDataType) -> Result<PathBuf, AppDirsError> {
    platform::get_app_dir(t)
}
