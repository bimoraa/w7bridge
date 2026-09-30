use std::{collections::BTreeMap, fs, path::Path};

use tempfile::tempdir;

use super::{Policy, PolicyError};
use crate::config::{CommandDefinition, ProjectDefinition};

fn project(root: &Path) -> ProjectDefinition {

    ProjectDefinition {

        id: "sample".into(),
        root: root.to_owned(),
        files: Default::default(),
        requires_sync: false,
        presets: None,
        git: None,
        commands: BTreeMap::from([(
            "test".into(),
            CommandDefinition {

                executable: std::env::current_exe().unwrap(),
                background: false,
                restart_on_sync: false,
                source_snapshot: false,
                args: vec!["비공개 인자".into()],
                env: BTreeMap::from([("TOKEN".into(), "비공개 값".into())]),

            },
        )]),

    }

}

#[test]
fn registry_rejects_duplicates_bad_names_relative_paths_and_unknown_commands() {

    let root = tempdir().unwrap();
    assert!(matches!(Policy::new(vec![project(root.path()), project(root.path())]), Err(PolicyError::Duplicate)));
    assert!(Policy::new(vec![project(Path::new("relative"))]).is_err());
    let mut invalid = project(root.path());
    invalid.id = "../escape".into();
    assert!(matches!(Policy::new(vec![invalid]), Err(PolicyError::Name)));
    let registry = Policy::new(vec![project(root.path())]).unwrap();
    assert!(matches!(registry.resolve("sample", "missing"), Err(PolicyError::Unknown)));

}

#[test]
fn listing_hides_execution_settings_and_deleted_paths_fail_closed() {

    let root = tempdir().unwrap();
    let directory = root.path().join("project");
    fs::create_dir(&directory).unwrap();
    let registry = Policy::new(vec![project(&directory)]).unwrap();
    assert_eq!(
        serde_json::to_value(registry.list()).unwrap(),
        serde_json::json!([{ "id": "sample", "commands": ["test"] }])
    );
    assert!(registry.resolve("sample", "test").is_ok());
    fs::remove_dir(directory).unwrap();
    assert!(matches!(registry.resolve("sample", "test"), Err(PolicyError::Changed)));

}

#[test]
fn invalid_arguments_and_environment_are_rejected() {

    let root = tempdir().unwrap();

    for (args, env) in [
        (vec!["bad\0arg".into()], BTreeMap::new()),
        (vec![], BTreeMap::from([("bad=name".into(), "value".into())])),
        (vec![], BTreeMap::from([("name".into(), "bad\0value".into())])),
    ] {

        let mut definition = project(root.path());
        let command = definition.commands.get_mut("test").unwrap();
        command.args = args;
        command.env = env;
        assert!(matches!(Policy::new(vec![definition]), Err(PolicyError::Command)));

    }

}

#[cfg(windows)]
#[test]
fn batch_files_are_not_executables() {

    let root = tempdir().unwrap();
    let executable = root.path().join("tool.cmd");
    fs::write(&executable, "@echo 실행 금지").unwrap();
    let mut definition = project(root.path());
    definition.commands.get_mut("test").unwrap().executable = executable;
    assert!(matches!(Policy::new(vec![definition]), Err(PolicyError::Executable)));

}
