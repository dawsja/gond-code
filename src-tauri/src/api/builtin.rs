//! The `app.*` commands that belong to the API itself.

use super::registry::{Def, NoInput, Registry, Risk};
use super::Services;

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new("app.version", "The app's version", Risk::Read),
        |_, _: NoInput| Ok(env!("CARGO_PKG_VERSION").to_string()),
    );
}
