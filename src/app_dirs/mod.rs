#![warn(missing_docs)]
//! *Put your app's data in the right place on every platform*
//! # Usage
//! ```
//! extern crate app_dirs;
//! use app_dirs::*;
//!
//! const APP_INFO: AppInfo = AppInfo{name: "CoolApp", author: "SuperDev"};
//!
//! fn main () {
//!     // Where should I store my app's per-user configuration data?
//!     println!("{:?}", get_app_root(AppDataType::UserConfig, &APP_INFO));
//!     // Windows: "%APPDATA%\SuperDev\CoolApp"
//!     //   (e.g.: "C:\Users\Rusty\AppData\Roaming\SuperDev\CoolApp")
//!     //   macOS: "$HOME/Library/Application Support/CoolApp"
//!     //   (e.g.: "/Users/Rusty/Library/Application Support/CoolApp")
//!     //    *nix: "$HOME/.config/CoolApp" (or "$XDG_CONFIG_HOME/CoolApp", if defined)
//!     //   (e.g.: "/home/rusty/.config/CoolApp")
//!
//!     // How about nested cache data?
//!     println!("{:?}", get_app_dir(AppDataType::UserCache, &APP_INFO, "cache/images"));
//!     // Windows: "%LOCALAPPDATA%\SuperDev\CoolApp\cache\images"
//!     //   (e.g.: "C:\Users\Rusty\AppData\Local\SuperDev\CoolApp\cache\images")
//!     //   macOS: "$HOME/Library/Caches/CoolApp/cache/images"
//!     //   (e.g.: "/Users/Rusty/Library/Caches/CoolApp/cache/images")
//!     //    *nix: "$HOME/.cache/CoolApp/cache/images"
//!     //          (or "$XDG_CACHE_HOME/CoolApp/cache/images", if defined)
//!     //   (e.g.: "/home/rusty/.cache/CoolApp/cache/images")
//!
//!     // Remove "get_" prefix to recursively create nonexistent directories:
//!     // app_root(AppDataType::UserConfig, &APP_INFO)
//!     // app_dir(AppDataType::UserCache, &APP_INFO, "cache/images")
//! }
//! ```
mod common;
pub use self::common::*;
mod imp;
pub use self::imp::*;
mod utils;
pub use self::utils::*;

#[cfg(test)]
mod tests {
    use self::AppDataType::*;
    use super::*;
    #[test]
    fn it_works() {
        let info = AppInfo {
            name: "Awesome App".into(),
            author: "Dedicated Dev".into(),
        };
        let path = "/.not-hidden/subfolder!/with?/uni.code/¡Olé!/";
        let types = [UserConfig, UserData, UserCache, SharedData, SharedConfig];
        for &t in types.iter() {
            println!("{:?} data root = {:?}", t, get_data_root(t));
            println!("{:?} app root = {:?}", t, get_app_root(t, &info));
            println!("{:?} data dir = {:?}", t, get_app_dir(t, &info, &path));
        }
    }
}
