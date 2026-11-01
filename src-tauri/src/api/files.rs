//! `file:line` links in a pane: `file.resolve` says which printed paths are real files (so only
//! those are links), `file.open` opens one at its line in your editor, `file.editors` names the
//! editors on this PC for Settings → General. (`registry::human_editing` keeps every other way
//! into an editor out.)

use schemars::JsonSchema;
use serde::Deserialize;

use super::registry::{Def, NoInput, Registry, Risk};
use super::Services;
use crate::editors::Editor;
use crate::error::AppResult;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveInput {
    /// Paths as a pane printed them (no `:line`).
    pub paths: Vec<String>,
    /// The pane's folder; relative paths resolve from it (else from your home folder).
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenInput {
    /// A file (`file.resolve`'s answer).
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "file.editors",
            "The code editors on this PC that a file:line link can open in",
            Risk::Read,
        ),
        |_, _: NoInput| -> AppResult<Vec<Editor>> { Ok(crate::editors::list()) },
    );
    r.add(
        Def::new(
            "file.resolve",
            "Which paths printed in a pane are files, resolved from its folder",
            Risk::Read,
        ),
        |_, i: ResolveInput| -> AppResult<Vec<Option<String>>> {
            let home = dirs::home_dir();
            let cwd = i.cwd.map(std::path::PathBuf::from);
            Ok(i.paths
                .iter()
                .map(|p| {
                    crate::editors::resolve(p, cwd.as_deref(), home.as_deref())
                        .map(|f| f.to_string_lossy().into_owned())
                })
                .collect())
        },
    );
    r.add(
        Def::new(
            "file.open",
            "Open a file at a line in your editor (Settings → General → Open files in)",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: OpenInput| -> AppResult<()> {
            let choice = crate::settings::get(&call.ctx.store)
                .ok()
                .and_then(|s| s.editor);
            crate::editors::open(
                std::path::Path::new(&i.path),
                i.line,
                i.column,
                choice.as_deref(),
            )
        },
    );
}
