use crate::{Config, security::Policy};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn approved_presets_detect_repo_type_and_explicit_commands_win( ) {

    let root = tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    for (manifest, content, expected) in [
        ("Cargo.toml", "[package]\nname='fixture'", vec!["build", "--locked"]),
        (
            "package.json",
            r#"{"scripts":{"check":"tsc","build":"vite build","test":"test","dev":"vite"}}"#,
            vec!["run", "build"],
        ),
    ] {

        fs::write(root.path().join(manifest), content).unwrap();
        let source = json!({"version":1,"projects":[{"id":"sample","root":root.path(),"files":{"enabled":true},"presets":{"executable":executable},
            "commands":{"test":{"executable":executable,"args":["explicit"]}}}]});
        let policy = Policy::new(Config::parse(&toml::to_string(&source).unwrap()).unwrap().projects).unwrap();
        assert_eq!(policy.resolve("sample", "build").unwrap().1.args, expected);
        assert_eq!(policy.resolve("sample", "build").unwrap().1.source_snapshot, manifest == "Cargo.toml");
        assert_eq!(policy.resolve("sample", "test").unwrap().1.args, ["explicit"]);
        assert!(policy.resolve("sample", "run").unwrap().1.background);
        fs::remove_file(root.path().join(manifest)).unwrap();

    }

}

#[test]
fn discovery_does_not_enable_presets_and_restart_requires_sync_background( ) {

    let root = tempdir().unwrap();
    fs::write(root.path().join("Cargo.toml"), "[package]").unwrap();
    let base = json!({"version":1,"projects":[{"id":"sample","root":root.path()}]});
    let policy = Policy::new(Config::parse(&toml::to_string(&base).unwrap()).unwrap().projects).unwrap();
    assert!(policy.resolve("sample", "build").is_err());
    for (background, requires_sync) in [(false, true), (true, false)] {

        let source = json!({"version":1,"projects":[{"id":"sample","root":root.path(),"requires_sync":requires_sync,"files":{"enabled":true},
            "commands":{"run":{"executable":std::env::current_exe().unwrap(),"background":background,"restart_on_sync":true}}}]});
        assert!(Policy::new(Config::parse(&toml::to_string(&source).unwrap()).unwrap().projects).is_err());

    }

}
