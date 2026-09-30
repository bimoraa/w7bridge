use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::error::PolicyError;
use serde::Serialize;

use crate::config::{CommandDefinition, ProjectDefinition};

#[derive(Serialize)]
pub(crate) struct ProjectSummary<'a> {

    id: &'a str,
    commands: Vec<&'a str>,

}

pub(crate) struct Policy {

    projects: BTreeMap<String, ProjectDefinition>,

}

impl Policy {

    pub fn new(definitions: Vec<ProjectDefinition>) -> Result<Self, PolicyError> {

        let mut projects = BTreeMap::new();

        for mut project in definitions {

            name(&project.id)?;
            project.root = canonical(&project.root, true)?;

            for (command_name, command) in &mut project.commands {

                name(command_name)?;
                command.executable = canonical(&command.executable, false)?;

                if command.args.iter().any(|arg| arg.contains('\0'))
                    || command
                        .env
                        .iter()
                        .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
                {

                    return Err(PolicyError::Command);

                }

            }

            if projects.insert(project.id.clone(), project).is_some() {

                return Err(PolicyError::Duplicate);

            }

        }

        Ok(Self { projects })

    }

    pub fn list(&self) -> Vec<ProjectSummary<'_>> {

        self.projects
            .values()
            .map(|project| ProjectSummary {

                id: &project.id,
                commands: project.commands.keys().map(String::as_str).collect(),

            })
            .collect()

    }

    pub fn resolve(&self, project_id: &str, command_name: &str) -> Result<(&Path, &CommandDefinition), PolicyError> {

        let project = self.projects.get(project_id).ok_or(PolicyError::Unknown)?;
        let command = project.commands.get(command_name).ok_or(PolicyError::Unknown)?;

        // 원래 canonical 경로를 계속 사용하고, 다른 경로로 바뀌었다면 실행을 막아.
        if canonical(&project.root, true).ok().as_ref() != Some(&project.root)
            || canonical(&command.executable, false).ok().as_ref() != Some(&command.executable)
        {

            return Err(PolicyError::Changed);

        }

        Ok((&project.root, command))

    }

}

fn name(value: &str) -> Result<(), PolicyError> {

    let valid = !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte));

    if valid { Ok(()) } else { Err(PolicyError::Name) }

}

fn canonical(path: &Path, directory: bool) -> Result<PathBuf, PolicyError> {

    let invalid = || {

        if directory { PolicyError::Root } else { PolicyError::Executable }

    };

    if !path.is_absolute() {

        return Err(invalid());

    }

    let resolved = path.canonicalize().map_err(|_| invalid())?;

    if (directory && !resolved.is_dir()) || (!directory && !resolved.is_file()) {

        return Err(invalid());

    }

    #[cfg(windows)]
    if !directory && !resolved.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("exe")) {

        return Err(invalid());

    }

    Ok(resolved)

}

#[cfg(test)]
#[path = "../tests/unit/policy.rs"]
mod tests;
