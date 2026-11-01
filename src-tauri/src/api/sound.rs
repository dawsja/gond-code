//! `sound.play`: a pane's agent finished or wants you, and its cue plays (`crate::sound`), unless
//! Settings → General → Sounds is off.

use schemars::JsonSchema;
use serde::Deserialize;

use super::registry::{Def, Registry, Risk};
use super::Services;
use crate::error::AppResult;
use crate::sound::Cue;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayInput {
    pub cue: Cue,
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "sound.play",
            "Play the cue for an agent that's done or needs you, if Sounds is on",
            Risk::WriteLocal,
        ),
        |call, i: PlayInput| -> AppResult<bool> {
            let on = crate::settings::get(&call.ctx.store).map_or(true, |s| s.sounds);
            Ok(on && crate::sound::play(i.cue))
        },
    );
}
