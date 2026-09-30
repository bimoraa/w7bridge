/*! 로컬 owner가 승인한 executable에 project별 기본 명령을 고정해. */

use crate::{
    PolicyError,
    config::{CommandDefinition, ProjectDefinition},
};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

pub(super) fn detect( root: &Path, ) -> Option<&'static str> {

    if regular(&root.join("Cargo.toml")) {

        Some("cargo")

    } else if regular(&root.join("package.json")) {

        Some("node")

    } else {

        None

    }

}

fn regular( path: &Path, ) -> bool {

    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !crate::filesystem::paths::redirected(&metadata))

}

pub(super) fn apply( project: &mut ProjectDefinition, ) -> Result<(), PolicyError> {

    let Some(settings) = &project.presets else { return Ok(()) };
    let kind =
        if settings.kind == "auto" { detect(&project.root).ok_or(PolicyError::Command)? } else { &settings.kind };
    let arguments: Vec<(&str, Vec<String>)> = match kind {

        "cargo" => ["check", "build", "test", "run"]
            .into_iter()
            .map(|name| (name, vec![name.into(), "--locked".into()]))
            .collect(),
        "node" => {

            let path = project.root.join("package.json");
            if !regular(&path) || fs::metadata(&path).map_err(|_| PolicyError::Command)?.len() > 1_048_576 {

                return Err(PolicyError::Command);

            }
            let package: Value = serde_json::from_slice(&fs::read(path).map_err(|_| PolicyError::Command)?)
                .map_err(|_| PolicyError::Command)?;
            let scripts = package["scripts"].as_object().ok_or(PolicyError::Command)?;
            ["check", "build", "test", "run"]
                .into_iter()
                .filter_map(|name| {

                    let script = if name == "run" && !scripts.contains_key("run") { "dev" } else { name };
                    scripts
                        .get(script)
                        .filter(|value| value.is_string())
                        .map(|_| (name, vec!["run".into(), script.into()]))

                })
                .collect()

        }
        _ => return Err(PolicyError::Command),

    };
    for (name, args) in arguments {

        project.commands.entry(name.into()).or_insert_with(|| CommandDefinition {

            executable: settings.executable.clone(),
            args: settings.prefix_args.iter().cloned().chain(args).collect(),
            env: BTreeMap::new(),
            background: name == "run",
            restart_on_sync: false,
            source_snapshot: kind == "cargo" && name != "run" && project.files.enabled,

        });

    }
    Ok(())

}

#[cfg(test)]
#[path = "../../tests/unit/presets.rs"]
mod tests;
