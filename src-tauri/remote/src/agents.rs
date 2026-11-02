//! The app's agent code the host runs. A `#[path]` here is from this file's folder: one in
//! an inline `mod agents { … }` would be from `src/agents/`, which doesn't exist, and Linux
//! won't walk `..` out of a folder that isn't there.

#[path = "../../src/agents/cwd.rs"]
pub mod cwd;
#[path = "../../src/agents/resume.rs"]
pub mod resume;
#[path = "../../src/agents/running.rs"]
pub mod running;
#[path = "../../src/agents/spec.rs"]
pub mod spec;
