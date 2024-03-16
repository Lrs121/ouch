//! Random and miscellaneous utils used in ouch.
//!
//! In here we have the logic for custom formatting, some file and directory utils, and user
//! stdin interaction helpers.

pub mod colors;
mod file_visibility;
mod formatting;
mod fs;
pub mod io;
pub mod logger;
mod question;

pub use file_visibility::FileVisibilityPolicy;
pub use formatting::{
    nice_directory_display, pretty_format_list_of_paths, strip_cur_dir, to_utf, Bytes, EscapedPathDisplay,
};
pub use fs::{
    cd_into_same_dir_as, clear_path, create_dir_if_non_existent, is_symlink, remove_file_or_dir, try_infer_extension,
};
pub use question::{
    ask_to_create_file, user_wants_to_continue, user_wants_to_overwrite, QuestionAction, QuestionPolicy,
};
pub use utf8::{get_invalid_utf8_paths, is_invalid_utf8};

mod utf8 {
    use std::{ffi::OsStr, path::PathBuf};

    /// Check, without allocating, if os_str can be converted into &str
    pub fn is_invalid_utf8(os_str: impl AsRef<OsStr>) -> bool {
        os_str.as_ref().to_str().is_none()
    }

    /// Filter out list of paths that are not utf8 valid
    pub fn get_invalid_utf8_paths(paths: &[PathBuf]) -> Vec<&PathBuf> {
        paths.iter().filter(|path| is_invalid_utf8(path)).collect()
    }
}
