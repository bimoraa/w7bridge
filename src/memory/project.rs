/*! registry가 허용한 project와 context 경로만 memory 도구에 연결해. */

use super::{context::ContextDocument, state::MemoryState, storage};
use crate::{FileError, filesystem::{FileStore, validate_paths}, security::Policy};
use serde::Serialize;

pub(crate) struct ProjectMemory {

    project_id: String,
    store: FileStore,
    context_files: Vec<String>,

}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectContext {

    pub project_id: String,
    pub context_files: Vec<String>,
    #[serde(flatten)]
    pub document: ContextDocument,

}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectRevision {

    pub project_id: String,
    pub path: String,
    #[serde(flatten)]
    pub state: MemoryState,

}

impl ProjectMemory {

    pub(crate) fn open( policy: &Policy, project_id: &str, ) -> Result<Self, FileError> {

        let store = policy.files(project_id)?;
        let mut context_files: Vec<String> = ["AGENTS.md", "MEMORY.md", "PLANS.md"]
            .into_iter().map(String::from).chain(store.settings().context_files.iter().cloned()).collect();
        context_files.sort();
        context_files.dedup();
        validate_paths(context_files.iter().map(String::as_str))?;
        Ok(Self { project_id: project_id.into(), store, context_files })

    }

    pub(crate) fn read( &self, path: &str, ) -> Result<ProjectContext, FileError> {

        self.validate(path)?;
        let document = storage::read(&self.store, path)?;
        Ok(ProjectContext { project_id: self.project_id.clone(), context_files: self.context_files.clone(), document })

    }

    pub(crate) fn update( &self, path: &str, content: &str, expected: Option<&str>, ) -> Result<ProjectRevision, FileError> {

        self.validate(path)?;
        let state = storage::update(&self.store, path, content, expected)?;
        Ok(ProjectRevision { project_id: self.project_id.clone(), path: path.into(), state })

    }

    fn validate( &self, path: &str, ) -> Result<(), FileError> {

        if !self.context_files.iter().any(|context| context == path) || !self.store.permits(path) {

            return Err(FileError::Path);

        }
        Ok(())

    }

}
