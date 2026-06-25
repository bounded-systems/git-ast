//! Configuration handling.
//!
//! Reads `git-ast` settings for a path from Git sources (`.gitattributes` and
//! git config). A real implementation would shell out to
//! `git check-attr filter diff merge -- <path>`; the function below is a
//! placeholder that infers settings from the file extension.

use crate::Error;

/// The resolved `git-ast` configuration for a single file path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileConfig {
    /// Whether the clean/smudge filter applies.
    pub use_filter: bool,
    /// Whether the custom diff driver applies.
    pub use_diff_driver: bool,
    /// Whether the custom merge driver applies.
    pub use_merge_driver: bool,
}

/// Determine the `git-ast` configuration for `path`.
///
/// Placeholder: treats common source extensions as fully managed and everything
/// else as passthrough. A real implementation would consult `.gitattributes`.
pub fn get_config_for_path(path: &str) -> Result<FileConfig, Error> {
    let managed = path.ends_with(".rs") || path.ends_with(".py") || path.ends_with(".js");
    if managed {
        Ok(FileConfig {
            use_filter: true,
            use_diff_driver: true,
            use_merge_driver: true,
        })
    } else {
        Ok(FileConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_files_are_managed() {
        let cfg = get_config_for_path("src/main.rs").unwrap();
        assert!(cfg.use_filter && cfg.use_diff_driver && cfg.use_merge_driver);
    }

    #[test]
    fn other_files_are_passthrough() {
        assert_eq!(
            get_config_for_path("README.md").unwrap(),
            FileConfig::default()
        );
    }
}
