//! OS-native application storage and first-run resource installation.

use std::io;
use std::path::{Path, PathBuf};

const RESOURCE_VERSION: &str = "early-classic-0.0.14a_08-1";

pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("FerrumCraft")
}

pub fn initialize() -> io::Result<PathBuf> {
    let app_root = app_data_dir();
    let bundle_root = bundled_resource_root().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not find bundled FerrumCraft assets and data",
        )
    })?;
    initialize_at(&app_root, &bundle_root)?;
    migrate_local_save(&app_root)?;
    Ok(app_root)
}

fn bundled_resource_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("FERRUMCRAFT_RESOURCE_ROOT") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        candidates.push(parent.to_path_buf());
        candidates.push(parent.join("resources"));
    }
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current);
    }

    candidates
        .into_iter()
        .find(|root| root.join("assets").is_dir() && root.join("data").is_dir())
}

fn initialize_at(app_root: &Path, bundle_root: &Path) -> io::Result<()> {
    std::fs::create_dir_all(app_root.join("saves"))?;
    std::fs::create_dir_all(app_root.join("logs"))?;

    let marker = app_root.join(".resources-version");
    let installed_version = std::fs::read_to_string(&marker).unwrap_or_default();
    if installed_version.trim() != RESOURCE_VERSION {
        copy_tree(&bundle_root.join("assets"), &app_root.join("assets"))?;
        copy_tree(&bundle_root.join("data"), &app_root.join("data"))?;
        std::fs::write(marker, RESOURCE_VERSION)?;
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else {
            std::fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

fn migrate_local_save(app_root: &Path) -> io::Result<()> {
    let destination = app_root.join("saves").join("world.json");
    let source = PathBuf::from("saves").join("world.json");
    if !destination.exists() && source.is_file() {
        std::fs::copy(source, destination)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_resources_into_application_data() {
        let unique = format!("ferrumcraft-storage-test-{}", std::process::id());
        let root = std::env::temp_dir().join(unique);
        let bundle = root.join("bundle");
        let app = root.join("app");
        std::fs::create_dir_all(bundle.join("assets/ferrumcraft")).unwrap();
        std::fs::create_dir_all(bundle.join("data/ferrumcraft")).unwrap();
        std::fs::write(bundle.join("assets/ferrumcraft/test.txt"), "asset").unwrap();
        std::fs::write(bundle.join("data/ferrumcraft/test.txt"), "data").unwrap();

        initialize_at(&app, &bundle).unwrap();

        assert_eq!(
            std::fs::read_to_string(app.join("assets/ferrumcraft/test.txt")).unwrap(),
            "asset"
        );
        assert_eq!(
            std::fs::read_to_string(app.join("data/ferrumcraft/test.txt")).unwrap(),
            "data"
        );
        assert!(app.join("saves").is_dir());
        assert!(app.join("logs").is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }
}
