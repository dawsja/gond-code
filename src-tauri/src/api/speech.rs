//! `speech.*` behind the registry (`crate::speech`): turn speech to text on (download the
//! model, with progress at `/speech`), off (delete it), and transcribe a recording.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use base64::Engine;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::registry::{Def, Registry, Risk};
use super::state::{Cause, StateTree};
use super::Services;
use crate::error::{AppError, AppResult};
use crate::store::{settings as repo, Store};

/// The settings key that says speech to text is on: off until you turn it on, whatever is
/// on disk.
const KEY: &str = "speech";

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoInput {}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscribeInput {
    /// 16 kHz mono samples as little-endian f32, base64.
    pub audio: String,
}

/// The download in flight, if any: stopping it is turning speech off.
static CANCEL: std::sync::Mutex<Option<Arc<AtomicBool>>> = std::sync::Mutex::new(None);

fn put(tree: &StateTree, v: serde_json::Value) {
    tree.commit(
        Cause::Runtime {
            runtime: "speech".into(),
        },
        move |d| d["speech"] = v,
    );
}

fn set_on(store: &Store, on: bool) {
    crate::speech::set_on(on);
    let _ = store.with(|c| repo::set(c, KEY, &serde_json::Value::Bool(on)));
}

/// Startup: on only if you turned it on and the model is here (a download Gond Code closed
/// in the middle of turns it off).
pub fn sync(s: &Services) {
    if let Some(why) = crate::speech::unsupported() {
        set_on(&s.store, false);
        put(&s.state, json!({ "state": "unsupported", "error": why }));
        return;
    }
    let wanted = s
        .store
        .with(repo::all)
        .ok()
        .and_then(|all| all.into_iter().find(|(k, _)| k == KEY))
        .is_some_and(|(_, v)| v == true);
    let on = wanted && crate::speech::model_path().is_some();
    set_on(&s.store, on);
    put(
        &s.state,
        json!({ "state": if on { "ready" } else { "off" } }),
    );
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "speech.enable",
            "Turn speech to text on: download the model (731 MB) to this PC",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, _: NoInput| -> AppResult<()> {
            let s = call.ctx;
            // Never download a model this PC can't run.
            if let Some(why) = crate::speech::unsupported() {
                return Err(AppError::Invalid(why));
            }
            if crate::speech::model_path().is_some() {
                set_on(&s.store, true);
                put(&s.state, json!({ "state": "ready" }));
                return Ok(());
            }
            let dir = crate::speech::dir()
                .ok_or_else(|| AppError::Invalid("no local app data folder".into()))?;
            let stop = Arc::new(AtomicBool::new(false));
            {
                let mut g = CANCEL.lock().unwrap_or_else(|e| e.into_inner());
                if g.is_some() {
                    return Ok(()); // already downloading
                }
                *g = Some(stop.clone());
            }
            put(&s.state, json!({ "state": "downloading", "percent": 0 }));
            let (tree, store) = (s.state.clone(), s.store.clone());
            std::thread::spawn(move || {
                let mut last = u64::MAX;
                let result = crate::speech::download(&dir, &stop, |done, total| {
                    let pct = done * 100 / total.max(1);
                    if pct != last {
                        last = pct;
                        put(&tree, json!({ "state": "downloading", "percent": pct }));
                    }
                });
                {
                    let mut g = CANCEL.lock().unwrap_or_else(|e| e.into_inner());
                    if g.as_ref().is_some_and(|c| Arc::ptr_eq(c, &stop)) {
                        *g = None;
                    }
                }
                if stop.load(Ordering::SeqCst) {
                    return; // turned off meanwhile: speech.disable said so
                }
                match result {
                    Ok(_) => {
                        set_on(&store, true);
                        put(&tree, json!({ "state": "ready" }));
                    }
                    Err(e) => put(&tree, json!({ "state": "error", "error": e.to_string() })),
                }
            });
            Ok(())
        },
    );
    r.add(
        Def::new(
            "speech.disable",
            "Turn speech to text off: stop any download and delete the model",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, _: NoInput| -> AppResult<()> {
            set_on(&call.ctx.store, false);
            // A download in flight deletes its own part file as it stops (Windows won't
            // delete a file that's open); otherwise the model goes now.
            match CANCEL.lock().unwrap_or_else(|e| e.into_inner()).take() {
                Some(stop) => stop.store(true, Ordering::SeqCst),
                None => crate::speech::remove()?,
            }
            put(&call.ctx.state, json!({ "state": "off" }));
            Ok(())
        },
    );
    r.add(
        Def::new(
            "speech.transcribe",
            "Text from a recording: 16 kHz mono f32 samples, base64",
            Risk::Read,
        )
        .human_only()
        .redact(&["/audio"]),
        |_, i: TranscribeInput| -> AppResult<String> {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(i.audio.trim())
                .map_err(|e| AppError::Invalid(format!("the recording isn't base64: {e}")))?;
            if bytes.len() % 4 != 0 {
                return Err(AppError::Invalid("the recording isn't f32 samples".into()));
            }
            let pcm: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            crate::speech::transcribe(&pcm)
        },
    );
}
