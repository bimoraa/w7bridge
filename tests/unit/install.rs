use super::*;
use std::ffi::OsString;
use tempfile::tempdir;

fn options(directory: &Path) -> InstallOptions {

    InstallOptions { directory: directory.into(), config: None, update: false }

}

#[test]
fn arguments_match_connect_defaults_and_reject_unsafe_or_duplicate_paths() {

    let defaults = InstallOptions::parse(&[]).unwrap();
    assert_eq!(defaults.directory, PathBuf::from("C:/w7bridge"));
    for arguments in [
        vec!["--dir", "relative"],
        vec!["--dir", "C:/Program Files/w7bridge"],
        vec!["--dir", "C:/w7bridge;whoami"],
        vec!["--dir", "C:/w7bridge/../other"],
        vec!["--dir", "C:/w7bridge/"],
        vec!["--dir", "C:/w7bridge."],
        vec!["--dir", "C:/w7bridge:stream"],
        vec!["--config"],
        vec!["--update", "--update"],
        vec!["--unknown"],
    ] {

        let args: Vec<_> = arguments.into_iter().map(OsString::from).collect();
        assert!(InstallOptions::parse(&args).is_err(), "{args:?}");

    }
    let args = ["--dir", "D:\\tools\\w7bridge", "--config", "settings file.toml", "--update"].map(OsString::from);
    let parsed = InstallOptions::parse(&args).unwrap();
    assert_eq!(parsed.config, Some("settings file.toml".into()));
    assert!(parsed.update);

}

#[test]
fn fresh_install_publishes_complete_files_and_preserves_unrelated_files() {

    let root = tempdir().unwrap();
    let source = root.path().join("source.exe");
    let directory = root.path().join("installed");
    fs::write(&source, b"server executable").unwrap();
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("keep.txt"), "keep").unwrap();
    install_files(&options(&directory), &source).unwrap();
    assert_eq!(fs::read(directory.join("w7bridge.exe")).unwrap(), b"server executable");
    Config::load(&directory.join("w7bridge.toml")).unwrap();
    assert_eq!(fs::read_to_string(directory.join("keep.txt")).unwrap(), "keep");
    assert!(!directory.join(".w7bridge-install").exists());
    assert!(matches!(install_files(&options(&directory), &source), Err(InstallError::Exists)));

}

#[test]
fn update_preserves_config_bytes_and_rejects_running_from_the_installed_path() {

    let root = tempdir().unwrap();
    let source = root.path().join("source.exe");
    let directory = root.path().join("installed");
    let mut options = options(&directory);
    fs::write(&source, "old").unwrap();
    install_files(&options, &source).unwrap();
    let config = "# keep my comments\r\nversion = 1\r\nprojects = []\r\n";
    fs::write(directory.join("w7bridge.toml"), config).unwrap();
    fs::write(&source, "new").unwrap();
    options.update = true;
    install_files(&options, &source).unwrap();
    assert_eq!(fs::read_to_string(directory.join("w7bridge.exe")).unwrap(), "new");
    assert_eq!(fs::read_to_string(directory.join("w7bridge.toml")).unwrap(), config);
    assert!(matches!(install_files(&options, &directory.join("w7bridge.exe")), Err(InstallError::SameExecutable)));
    options.config = Some(source.clone());
    assert!(matches!(install_files(&options, &source), Err(InstallError::ConfigExists)));

}

#[test]
fn invalid_config_or_registry_is_rejected_before_any_file_is_installed() {

    let root = tempdir().unwrap();
    let source = root.path().join("source.exe");
    let config = root.path().join("source.toml");
    let directory = root.path().join("installed");
    let mut options = options(&directory);
    options.config = Some(config.clone());
    fs::write(&source, "binary").unwrap();
    fs::write(&config, "version = 2").unwrap();
    assert!(matches!(install_files(&options, &source), Err(InstallError::Config(_))));
    assert!(!directory.exists());
    fs::write(&config, "version = 1\n[[projects]]\nid = 'sample'\nroot = 'relative'").unwrap();
    assert!(matches!(install_files(&options, &source), Err(InstallError::Policy(_))));
    assert!(!directory.exists());
    fs::write(&config, "version = 1\nprojects = []\n# custom config").unwrap();
    install_files(&options, &source).unwrap();
    assert_eq!(fs::read(directory.join("w7bridge.toml")).unwrap(), fs::read(config).unwrap());

}

#[test]
fn missing_source_leaves_existing_install_untouched_and_releases_staging() {

    let root = tempdir().unwrap();
    let directory = root.path().join("installed");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("w7bridge.exe"), "old").unwrap();
    let config = "version = 1\nprojects = []\n";
    fs::write(directory.join("w7bridge.toml"), config).unwrap();
    let mut options = options(&directory);
    options.update = true;
    assert!(install_files(&options, &root.path().join("missing.exe")).is_err());
    assert_eq!(fs::read_to_string(directory.join("w7bridge.exe")).unwrap(), "old");
    assert_eq!(fs::read_to_string(directory.join("w7bridge.toml")).unwrap(), config);
    assert!(!directory.join(".w7bridge-install").exists());

}

#[test]
fn existing_staging_is_preserved_without_publishing_files() {

    let root = tempdir().unwrap();
    let directory = root.path().join("installed");
    let source = root.path().join("source.exe");
    fs::write(&source, "binary").unwrap();
    fs::create_dir(&directory).unwrap();
    let staging = directory.join(".w7bridge-install");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("keep"), "previous operation").unwrap();
    assert!(install_files(&options(&directory), &source).is_err());
    assert_eq!(fs::read_to_string(staging.join("keep")).unwrap(), "previous operation");
    assert!(!directory.join("w7bridge.exe").exists());
    assert!(!directory.join("w7bridge.toml").exists());

}

#[test]
fn failed_binary_copy_removes_only_owned_staging_files() {

    let root = tempdir().unwrap();
    let directory = root.path().join("installed");
    assert!(install_files(&options(&directory), root.path()).is_err());
    assert!(!directory.join(".w7bridge-install").exists());
    assert!(!directory.join("w7bridge.exe").exists());
    assert!(!directory.join("w7bridge.toml").exists());

}

#[cfg(unix)]
#[test]
fn symlink_destination_does_not_modify_its_target() {

    let root = tempdir().unwrap();
    let source = root.path().join("source.exe");
    let directory = root.path().join("installed");
    fs::write(&source, "binary").unwrap();
    fs::create_dir(&directory).unwrap();
    std::os::unix::fs::symlink(&source, directory.join("w7bridge.exe")).unwrap();
    let mut options = options(&directory);
    options.update = true;
    assert!(install_files(&options, &source).is_err());
    assert_eq!(fs::read_to_string(source).unwrap(), "binary");
    assert!(!directory.join("w7bridge.toml").exists());

}
