//! Windows provides three different ways to get the paths to roaming and local
//! app data: environment variables, KNOWNFOLDERID, and CSIDL. From the CSIDL
//! documentation:
//!
//! *"These values supersede the use of environment variables for this purpose.
//! They are in turn superseded in Windows Vista and later by the KNOWNFOLDERID
//! values."*
//! - https://msdn.microsoft.com/en-us/library/windows/desktop/bb762494.aspx
//!
//! -_-

// The function get_folder_path was adapted from:
// https://github.com/AndyBarron/preferences-rs/blob/f03c7/src/lib.rs#L211-L296
//
// Credit for the above code goes to Connorcpu (https://github.com/Connorcpu).

extern crate winapi;
use app_dirs::AppDataType::*;
use app_dirs::common::*;
use self::winapi::um::shlobj::SHGetKnownFolderPath;
use self::winapi::um::combaseapi::CoTaskMemFree;
use self::winapi::shared::guiddef::GUID;
use self::winapi::shared::ntdef::PWSTR;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr;
use std::slice;

pub const USE_AUTHOR: bool = true;

pub fn get_app_dir(t: AppDataType) -> Result<PathBuf, AppDirsError> {
    let folder_id = match t {
        UserConfig | UserData | SharedConfig | SharedData => &FOLDERID_RoamingAppData,
        UserCache => &FOLDERID_LocalAppData,
    };
    get_folder_path(folder_id).map(|os_str| os_str.into())
}

/// https://msdn.microsoft.com/en-us/library/dd378457.aspx#FOLDERID_RoamingAppData
#[allow(non_upper_case_globals)]
static FOLDERID_RoamingAppData: GUID = GUID {
    Data1: 0x3EB685DB,
    Data2: 0x65F9,
    Data3: 0x4CF6,
    Data4: [0xA0, 0x3A, 0xE3, 0xEF, 0x65, 0x72, 0x9F, 0x3D],
};

/// https://msdn.microsoft.com/en-us/library/dd378457.aspx#FOLDERID_LocalAppData
#[allow(non_upper_case_globals)]
static FOLDERID_LocalAppData: GUID = GUID {
    Data1: 0xF1B32785,
    Data2: 0x6FBA,
    Data3: 0x4FCF,
    Data4: [0x9D, 0x55, 0x7B, 0x8E, 0x7F, 0x15, 0x70, 0x91],
};

/// Wrapper around `winapi::PWSTR` to automatically free the string pointer.
/// This ensures the memory is freed when `get_folder_path` scope is left,
/// regardless of whether the call succeeded or failed/panicked.
struct SafePwstr(PWSTR);
impl Drop for SafePwstr {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(self.0 as *mut _) }
    }
}

fn get_folder_path(folder_id: &GUID) -> Result<OsString, AppDirsError> {
    unsafe {
        // Wide C string to be allocated by SHGetKnownFolderPath.
        // We are responsible for freeing this!
        let mut raw_path: PWSTR = ptr::null_mut();

        // SHGetKnownFolderPath arguments:
        // 1. reference to KNOWNFOLDERID
        // 2. no flags
        // 3. null handle -> current user
        // 4. output location
        let result = SHGetKnownFolderPath(folder_id, 0, ptr::null_mut(), &mut raw_path);

        // SHGetKnownFolderPath shouldn't ever fail, but if it does,
        // it will return a negative HRESULT.
        if result < 0 {
            return Err(AppDirsError::NotSupported);
        }

        // Ensures that the PWSTR is free when we leave this scope through
        // normal execution or a thread panic.
        let _cleanup = SafePwstr(raw_path);

        // Manually calculate length of wide C string.
        let mut length = 0;
        for i in 0.. {
            if *raw_path.offset(i) == 0 {
                length = i as usize;
                break;
            }
        }

        let wpath: &[u16] = slice::from_raw_parts(raw_path, length);
        let path: OsString = OsStringExt::from_wide(wpath);
        Ok(path)
        // _cleanup is deallocated, so raw_path is freed
    }
}
