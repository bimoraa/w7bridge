use super::*;
use tempfile::tempdir;

fn store(root: &Path) -> FileStore {

    FileStore::new(root, FileSettings { enabled: true, ..Default::default() }).unwrap()

}

#[test]
fn context_is_shared_even_when_gitignored_and_artifacts_are_filtered() {

    let root = tempdir().unwrap();
    let files = store(root.path());
    for name in ["AGENTS.md", "MEMORY.md", "PLANS.md", "notes/context.md", ".gitignore", ".env.example"] {

        files.write(name, Some(b"context"), None).unwrap();

    }
    fs::write(root.path().join(".gitignore"), "MEMORY.md\nnotes/\n").unwrap();
    for directory in [".git", "target", "node_modules", "dist", ".cache", ".next"] {

        fs::create_dir(root.path().join(directory)).unwrap();
        fs::write(root.path().join(directory).join("MEMORY.md"), "excluded").unwrap();
        assert!(matches!(files.read(&format!("{directory}/MEMORY.md")), Err(FileError::Path)));
        assert!(matches!(files.write(&format!("{directory}/new.md"), Some(b"blocked"), None), Err(FileError::Path)));

    }
    fs::write(root.path().join("tool.exe"), "excluded").unwrap();
    fs::write(root.path().join(".env"), "excluded").unwrap();
    let listing = files.list().unwrap();
    assert_eq!(listing.len(), 6);
    assert!(listing.iter().any(|entry| entry.path == "MEMORY.md"));
    assert!(listing.iter().any(|entry| entry.path == "notes/context.md"));
    let invalid = FileSettings {

        enabled: true,
        exclude_dirs: vec!["notes".into()],
        context_files: vec!["notes/context.md".into()],
        ..Default::default()

    };
    assert!(invalid.validate().is_err());
    assert!(FileSettings { exclude_dirs: vec!["MEMORY.md".into()], ..Default::default() }.validate().is_err());

}

#[test]
fn conditional_writes_preserve_newer_content_and_deletions_require_the_version() {

    let root = tempdir().unwrap();
    let files = store(root.path());
    let first = files.write("notes/context.md", Some(b"first"), None).unwrap().unwrap();
    assert!(matches!(files.write("notes/context.md", Some(b"overwrite"), None), Err(FileError::Conflict)));
    let second = files.write("notes/context.md", Some(b"second"), Some(&first)).unwrap().unwrap();
    assert!(matches!(files.write("notes/context.md", None, Some(&first)), Err(FileError::Conflict)));
    assert_eq!(files.read("notes/context.md").unwrap(), b"second");
    files.write("notes/context.md", None, Some(&second)).unwrap();
    assert!(!root.path().join("notes/context.md").exists());
    let limited =
        files.with_settings(FileSettings { enabled: true, max_file_bytes: 1_048_576, ..Default::default() }).unwrap();
    assert!(matches!(limited.write("large.txt", Some(&vec![0; 1_048_577]), None), Err(FileError::Limit)));
    let _lock = files.lock("access.lock").unwrap();
    assert!(matches!(files.list(), Err(FileError::Busy)));

}

#[test]
fn unsafe_and_nonportable_paths_are_rejected() {

    let root = tempdir().unwrap();
    let files = store(root.path());
    for path in
        ["", "../escape", "/absolute", "a/../b", "C:/file", "a\\file", "NUL.txt", "COM1", "name.", "name ", "a//b"]
    {

        assert!(matches!(files.write(path, Some(b"no"), None), Err(FileError::Path)), "{path}");

    }
    assert!(FileStore::new(root.path(), FileSettings::default()).is_err());

}

#[cfg(unix)]
#[test]
fn symlink_files_parents_and_metadata_never_escape_the_root() {

    use std::os::unix::fs::symlink;
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("secret.md"), "private").unwrap();
    symlink(outside.path(), root.path().join("link")).unwrap();
    let files = store(root.path());
    assert!(matches!(files.list(), Err(FileError::Path)));
    assert!(matches!(files.read("link/secret.md"), Err(FileError::Path)));
    assert!(matches!(files.write("link/new.md", Some(b"no"), None), Err(FileError::Path)));
    fs::remove_dir_all(root.path().join(".w7bridge")).unwrap();
    symlink(outside.path(), root.path().join(".w7bridge")).unwrap();
    assert!(matches!(files.list(), Err(FileError::Path)));

}

#[test]
fn root_identity_is_persistent_and_new_folders_do_not_reuse_it() {

    let root = tempdir().unwrap();
    let other = tempdir().unwrap();
    let files = store(root.path());
    let key = files.root_key().unwrap();
    assert_eq!(key.len(), 64);
    assert_eq!(store(root.path()).root_key().unwrap(), key);
    assert_ne!(store(other.path()).root_key().unwrap(), key);
    let changed = files
        .with_settings(FileSettings { enabled: true, exclude_dirs: vec!["generated".into()], ..Default::default() })
        .unwrap();
    assert_eq!(changed.root_key().unwrap(), key);
    assert!(!changed.permits("generated/file.md"));
    assert!(changed.permits("MEMORY.md"));
    assert!(files.with_settings(FileSettings::default()).is_err());
    fs::write(files.metadata_dir().unwrap().join("root-key"), "corrupt").unwrap();
    assert!(matches!(files.root_key(), Err(FileError::Data)));

}

#[test]
fn case_and_file_directory_collisions_fail_before_transfer() {

    for paths in [vec!["src/File.rs", "src/file.rs"], vec!["Src/a.rs", "src/b.rs"], vec!["notes", "notes/context.md"]] {

        assert!(crate::filesystem::validate_paths(paths).is_err());

    }
    assert!(crate::filesystem::validate_paths(["notes/context.md", "notes/context.md", "notes/plan.md"]).is_ok());

}

#[cfg(windows)]
#[test]
fn windows_junctions_cannot_redirect_file_access_or_metadata() {

    let directory = std::env::current_dir().unwrap();
    let root = tempfile::tempdir_in(&directory).unwrap();
    let outside = tempfile::tempdir_in(&directory).unwrap();
    fs::write(outside.path().join("secret.md"), "private").unwrap();
    let files = store(root.path());
    let shell =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    for name in ["link", ".w7bridge"] {

        let link = root.path().join(name);
        let link = link.to_str().unwrap().trim_start_matches(r"\\?\");
        let target = outside.path().to_str().unwrap().trim_start_matches(r"\\?\");
        let output = std::process::Command::new(&shell)
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null",
                link.replace('\'', "''"),
                target.replace('\'', "''"),
            ))
            .output()
            .unwrap();
        assert!(output.status.success(), "junction={name}, link={link}, target={target}: {output:?}");
        assert!(matches!(files.list(), Err(FileError::Path)));
        assert!(matches!(files.read("link/secret.md"), Err(FileError::Path)));
        assert!(matches!(files.write("link/new.md", Some(b"no"), None), Err(FileError::Path)));
        assert_eq!(fs::read_to_string(outside.path().join("secret.md")).unwrap(), "private");
        assert!(!outside.path().join("new.md").exists());
        if name == "link" {

            fs::remove_dir(root.path().join("link")).unwrap();
            fs::remove_dir_all(root.path().join(".w7bridge")).unwrap();

        } else {

            fs::remove_dir(root.path().join(name)).unwrap();

        }

    }

}
