//! Fake-binary helpers shared by adapter unit tests. Included from `lib.rs`
//! under `#[cfg(all(test, unix))]`; not an integration test target.

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// Creates a unique scratch directory for a fake-binary test.
pub(crate) fn test_dir(name: &str) -> PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("fluxguard-{name}-{suffix}"));
    std::fs::create_dir_all(&root).expect("create test directory");
    root
}

/// Writes `contents` to `path` and marks it executable.
pub(crate) fn write_executable(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write test process");
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("make executable");
}
