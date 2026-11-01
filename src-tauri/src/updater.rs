//! In-app updates. On start, every few hours, and when you press Check for updates, the app
//! reads `latest.json` from the latest published release of `dawsja/gond-code`. What it finds
//! is mirrored to `/update` for Settings → Updates and the sidebar. Restart to update
//! (`app.update.install`) downloads the installer with its progress in `/update`, checks its
//! minisign signature against the key in tauri.conf.json, then runs it (NSIS, passive), which
//! closes the app and opens the new version; the sessions and their panes come back as saved.
//! Nothing installs without your click.
//!
//! For testing an update locally: `GOND_UPDATE_ENDPOINT` replaces the endpoint (a local
//! `latest.json`), and `GOND_UPDATE_AUTO=1` installs as soon as one is found. The signature
//! check still applies, so only an installer signed with our key installs.
//!
//! Gond Code Dev (`channel`) never updates itself: it would install the released Gond Code over
//! a dev build. `/update` says `channel: "dev"`, and check and install answer `invalid`.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

use crate::api::registry::{Def, NoInput, Registry, Risk};
use crate::api::state::Cause;
use crate::api::Services;
use crate::error::{AppError, AppResult};

/// How often a running app looks again.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

static APP: OnceLock<(AppHandle, Arc<Services>)> = OnceLock::new();

fn updater(app: &AppHandle) -> Result<Updater, String> {
    let mut b = app.updater_builder();
    if let Ok(url) = std::env::var("GOND_UPDATE_ENDPOINT") {
        let url = url
            .parse()
            .map_err(|e| format!("GOND_UPDATE_ENDPOINT: {e}"))?;
        b = b.endpoints(vec![url]).map_err(|e| e.to_string())?;
    }
    b.build().map_err(|e| e.to_string())
}

/// Changes `/update`: `{ current, phase, checkedAt?, available?, percent?, error? }`, where
/// phase is idle, checking, downloading or installing.
fn set(services: &Services, f: impl FnOnce(&mut Value)) {
    let cause = Cause::Runtime {
        runtime: "update".into(),
    };
    services.state.commit(cause, |d| {
        if !d["update"].is_object() {
            d["update"] = initial();
        }
        f(&mut d["update"]);
    });
}

fn plain(e: impl ToString) -> String {
    let e = e.to_string();
    if e.contains("error sending request") || e.contains("dns") || e.contains("connect") {
        "Couldn't reach GitHub. Check your connection and try again.".into()
    } else if e.contains("valid release JSON") {
        "Couldn't read the latest release from GitHub. Try again later.".into()
    } else {
        e
    }
}

/// Looks once and mirrors the answer to `/update`.
async fn check(app: &AppHandle, services: &Services) -> Result<Option<Update>, String> {
    set(services, |u| {
        u["phase"] = json!("checking");
        u.as_object_mut().map(|o| o.remove("error"));
    });
    let found = match updater(app) {
        Ok(up) => up.check().await.map_err(plain),
        Err(e) => Err(e),
    };
    set(services, |u| {
        u["phase"] = json!("idle");
        u["checkedAt"] = json!(crate::store::now());
        match &found {
            Ok(Some(up)) => {
                u["available"] = json!({
                    "version": up.version,
                    "notes": up.body,
                    "date": up.date.map(|d| d.to_string()),
                });
            }
            Ok(None) => {
                u.as_object_mut().map(|o| o.remove("available"));
            }
            Err(e) => u["error"] = json!(e),
        }
    });
    found
}

/// Downloads and verifies the newer installer with its progress in `/update`, then runs it. The
/// app closes for the installer, which opens the new version when it's done.
async fn install(app: &AppHandle, services: &Services) -> Result<(), String> {
    let Some(update) = check(app, services).await? else {
        return Err("Gond Code is already up to date.".into());
    };
    eprintln!("update: {} → {}", update.current_version, update.version);
    set(services, |u| {
        u["phase"] = json!("downloading");
        u["percent"] = json!(0);
    });
    let mut got = 0u64;
    let mut shown = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                got += chunk as u64;
                let Some(total) = total.filter(|t| *t > 0) else {
                    return;
                };
                let percent = (got * 100 / total).min(100);
                if percent != shown {
                    shown = percent;
                    set(services, |u| u["percent"] = json!(percent));
                }
            },
            || {},
        )
        .await
        .map_err(plain)?;
    set(services, |u| {
        u["phase"] = json!("installing");
        u["percent"] = json!(100);
    });
    update.install(bytes).map_err(|e| e.to_string())?;
    // Windows closes the app for its installer before this; elsewhere, start the new version.
    app.restart();
}

fn failed(services: &Services, e: String) {
    eprintln!("update: {e}");
    set(services, |u| {
        u["phase"] = json!("idle");
        u["error"] = json!(e);
        u.as_object_mut().map(|o| o.remove("percent"));
    });
}

/// `/update` before anything has been looked for.
fn initial() -> Value {
    json!({
        "current": env!("CARGO_PKG_VERSION"),
        "phase": "idle",
        "channel": crate::channel::CHANNEL.id(),
    })
}

/// Starts the background checks. Called once from the app's setup. Gond Code Dev doesn't look.
pub fn start(app: &AppHandle, services: Arc<Services>) {
    let _ = APP.set((app.clone(), services.clone()));
    set(&services, |_| {});
    if crate::channel::is_dev() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let auto = std::env::var("GOND_UPDATE_AUTO").is_ok_and(|v| v == "1");
        loop {
            match tauri::async_runtime::block_on(check(&app, &services)) {
                Ok(Some(_)) if auto => {
                    if let Err(e) = tauri::async_runtime::block_on(install(&app, &services)) {
                        failed(&services, e);
                    }
                }
                Ok(_) => {}
                // Offline or the endpoint is down: try again later, quietly.
                Err(e) => eprintln!("update check: {e}"),
            }
            std::thread::sleep(EVERY);
        }
    });
}

fn running() -> AppResult<(AppHandle, Arc<Services>)> {
    if crate::channel::is_dev() {
        return Err(AppError::Invalid(
            "Gond Code Dev doesn't update itself: pull and build it again".into(),
        ));
    }
    APP.get()
        .cloned()
        .ok_or_else(|| AppError::Invalid("updates run in the Gond Code app".into()))
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "app.update.status",
            "This version, the newer one available if any, and how a check or download is going",
            Risk::Read,
        ),
        |call, _: NoInput| -> AppResult<Value> {
            Ok(call
                .ctx
                .state
                .get("/update")
                .map(|(_, v)| v)
                .unwrap_or_else(initial))
        },
    );
    r.add(
        Def::new(
            "app.update.check",
            "Look for a newer Gond Code now; the answer lands in /update",
            Risk::Outward,
        )
        .human_only(),
        |_, _: NoInput| -> AppResult<()> {
            let (app, services) = running()?;
            tauri::async_runtime::spawn(async move {
                let _ = check(&app, &services).await;
            });
            Ok(())
        },
    );
    r.add(
        Def::new(
            "app.update.install",
            "Download the newer Gond Code, then restart into it where you left off",
            Risk::Outward,
        )
        .human_only(),
        |_, _: NoInput| -> AppResult<()> {
            let (app, services) = running()?;
            tauri::async_runtime::spawn(async move {
                if let Err(e) = install(&app, &services).await {
                    failed(&services, e);
                }
            });
            Ok(())
        },
    );
}
