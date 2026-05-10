// Copyright 2026 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fs;
use std::path::PathBuf;

use jj_lib::secure_config;
use tracing::instrument;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::ui::Ui;

/// Find and optionally delete repo-level config directories whose repo path
/// no longer exists.
#[derive(clap::Args, Clone, Debug)]
pub struct ConfigGcArgs {}

#[instrument(skip_all)]
pub async fn cmd_config_gc(
    ui: &mut Ui,
    command: &CommandHelper,
    _args: &ConfigGcArgs,
) -> Result<(), CommandError> {
    let root = command
        .config_env()
        .repo_configs_root_dir()
        .ok_or_else(|| user_error("No config directory found"))?;

    let missing = find_missing_repo_configs(&root)?;

    {
        let mut formatter = ui.stdout_formatter();
        writeln!(
            formatter,
            "Missing repo configs (repo path no longer exists):"
        )?;
        if missing.is_empty() {
            writeln!(formatter, "  (none)")?;
        } else {
            for (config_dir, repo_path) in &missing {
                writeln!(formatter, "  {}", config_dir.display())?;
                writeln!(formatter, "    path: {}", repo_path.display())?;
            }
        }
    }

    if missing.is_empty() {
        return Ok(());
    }

    let prompt = format!("Delete {} missing repo config directories?", missing.len());
    if !ui.prompt_yes_no(&prompt, Some(false))? {
        writeln!(ui.status(), "Aborted; nothing was deleted.")?;
        return Ok(());
    }

    for (config_dir, _) in &missing {
        match fs::remove_dir_all(config_dir) {
            Ok(()) => writeln!(ui.status(), "Deleted {}", config_dir.display())?,
            Err(err) => writeln!(
                ui.warning_default(),
                "Failed to delete {}: {err}",
                config_dir.display()
            )?,
        }
    }
    Ok(())
}

/// Returns `(config_dir, repo_path)` pairs for every per-repo config
/// directory under `root` whose recorded repo path no longer exists on
/// disk. The list is sorted by config directory name.
pub(crate) fn find_missing_repo_configs(
    root: &std::path::Path,
) -> Result<Vec<(PathBuf, PathBuf)>, CommandError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut dir_entries: Vec<_> = fs::read_dir(root)
        .map_err(|err| user_error(format!("Failed to read {}: {err}", root.display())))?
        .collect::<Result<_, _>>()
        .map_err(|err| user_error(format!("Failed to read {}: {err}", root.display())))?;
    dir_entries.sort_by_key(|e| e.file_name());

    let mut missing = Vec::new();
    for dir_entry in dir_entries {
        let config_dir = dir_entry.path();
        if !config_dir.is_dir() {
            continue;
        }
        let Ok(metadata) = secure_config::read_metadata(&config_dir) else {
            continue;
        };
        let Ok(Some(repo_path)) = secure_config::metadata_path(&metadata) else {
            continue;
        };
        if !repo_path.exists() {
            missing.push((config_dir, repo_path));
        }
    }
    Ok(missing)
}
