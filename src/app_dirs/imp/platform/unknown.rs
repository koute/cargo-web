use app_dirs::common::{AppDataType, AppDirsError};
use std::path::PathBuf;

pub const USE_AUTHOR: bool = false;

pub fn get_app_dir(_t: AppDataType) -> Result<PathBuf, AppDirsError> {
    Err(AppDirsError::NotSupported)
}
