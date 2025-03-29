//! Metadata that can be restored without changing ownership, ACLs or storage features.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs::{File, Metadata};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreservedMetadata {
    #[cfg(windows)]
    pub creation_time: u64,
    #[cfg(windows)]
    pub access_time: u64,
    #[cfg(windows)]
    pub write_time: u64,
    #[cfg(windows)]
    pub attributes: u32,
    #[cfg(not(windows))]
    pub accessed: std::time::SystemTime,
    #[cfg(not(windows))]
    pub modified: std::time::SystemTime,
    #[cfg(unix)]
    pub mode: u32,
}

#[cfg(windows)]
pub const SUPPORTED_ATTRIBUTES: u32 = {
    use windows_sys::Win32::Storage::FileSystem::*;
    FILE_ATTRIBUTE_READONLY
        | FILE_ATTRIBUTE_HIDDEN
        | FILE_ATTRIBUTE_SYSTEM
        | FILE_ATTRIBUTE_ARCHIVE
        | FILE_ATTRIBUTE_NORMAL
        | FILE_ATTRIBUTE_TEMPORARY
        | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED
};

impl PreservedMetadata {
    pub fn capture(metadata: &Metadata) -> Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            Ok(Self {
                creation_time: metadata.creation_time(),
                access_time: metadata.last_access_time(),
                write_time: metadata.last_write_time(),
                attributes: metadata.file_attributes(),
            })
        }
        #[cfg(not(windows))]
        {
            #[cfg(unix)]
            use std::os::unix::fs::PermissionsExt;
            Ok(Self {
                accessed: metadata.accessed()?,
                modified: metadata.modified()?,
                #[cfg(unix)]
                mode: metadata.permissions().mode(),
            })
        }
    }

    pub fn validate(&self) -> Result<()> {
        #[cfg(windows)]
        {
            ensure!(
                self.attributes & !SUPPORTED_ATTRIBUTES == 0,
                "Cannot preserve storage/cloud file attributes {:#x}; this file requires a different copy mechanism",
                self.attributes & !SUPPORTED_ATTRIBUTES
            );
            ensure!(
                [self.creation_time, self.access_time, self.write_time]
                    .iter()
                    .all(|t| *t > 0 && *t <= i64::MAX as u64),
                "Cannot preserve an unset or out-of-range Windows file timestamp"
            );
        }
        Ok(())
    }

    pub(crate) fn apply(&self, file: &File) -> Result<()> {
        self.validate()?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_BASIC_INFO, FileBasicInfo, SetFileInformationByHandle,
            };
            let info = FILE_BASIC_INFO {
                CreationTime: self.creation_time as i64,
                LastAccessTime: self.access_time as i64,
                LastWriteTime: self.write_time as i64,
                // NTFS change time should reflect this operation, rather than be forged from the source.
                ChangeTime: 0,
                FileAttributes: self.attributes,
            };
            // The caller owns FILE_WRITE_ATTRIBUTES access on a non-reparse file handle.
            let ok = unsafe {
                SetFileInformationByHandle(
                    file.as_raw_handle(),
                    FileBasicInfo,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32,
                )
            };
            ensure!(
                ok != 0,
                "Restore file timestamps and attributes: {}",
                std::io::Error::last_os_error()
            );
        }
        #[cfg(not(windows))]
        {
            file.set_times(
                std::fs::FileTimes::new()
                    .set_accessed(self.accessed)
                    .set_modified(self.modified),
            )?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(std::fs::Permissions::from_mode(self.mode))?;
            }
        }
        Ok(())
    }
}

/// Disable deferred access-time updates caused by hashing/copying through this handle.
pub(crate) fn freeze_access_time(file: &File) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::FILETIME, Storage::FileSystem::SetFileTime};
        let unchanged = FILETIME {
            dwLowDateTime: u32::MAX,
            dwHighDateTime: u32::MAX,
        };
        let ok = unsafe {
            SetFileTime(
                file.as_raw_handle(),
                std::ptr::null(),
                &unchanged,
                std::ptr::null(),
            )
        };
        ensure!(
            ok != 0,
            "Prevent access-time changes while reading: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(not(windows))]
    let _ = file;
    Ok(())
}
