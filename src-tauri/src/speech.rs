//! Speech to text, optional: NVIDIA's Parakeet Unified English 0.6B,
//! run on this PC through transcribe.cpp. Turning it on in Settings downloads the model
//! (`%LOCALAPPDATA%\GondCode\speech`); turning it off deletes it, and so does uninstalling
//! Gond Code (`windows/hooks.nsh`). The chat box records 16 kHz mono and sends it here.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// The model: Q8_0 of `handy-computer/parakeet-unified-en-0.6b-gguf`, pinned to a revision
/// and checked by its hash (the file Handy ships).
pub const FILE: &str = "parakeet-unified-en-0.6b-Q8_0.gguf";
const URL: &str = "https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/7e948f21b7bdbac698d3318db9d350f1096f3b6c/parakeet-unified-en-0.6b-Q8_0.gguf";
pub const BYTES: u64 = 731_357_568;
const SHA256: &str = "4b50b6dd862bf6e346929aaf4f5eaacec003bfa3f56462d6c874b41ef2f38795";

/// Why this PC can't run speech to text, if it can't: the engine is built for an x86 floor of
/// AVX2, FMA, F16C and BMI2 (`src-tauri/.cargo/config.toml`), and running it on a CPU without
/// them is an illegal instruction, a crash. Checked before the model is downloaded or loaded.
pub fn unsupported() -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        let missing: Vec<&str> = [
            ("AVX2", std::arch::is_x86_feature_detected!("avx2")),
            ("FMA", std::arch::is_x86_feature_detected!("fma")),
            ("F16C", std::arch::is_x86_feature_detected!("f16c")),
            ("BMI2", std::arch::is_x86_feature_detected!("bmi2")),
        ]
        .into_iter()
        .filter(|(_, has)| !has)
        .map(|(name, _)| name)
        .collect();
        if !missing.is_empty() {
            return Some(format!(
                "This PC's processor can't run speech to text: it doesn't have {} (processors from about 2013 on do).",
                missing.join(", ")
            ));
        }
    }
    None
}

/// Where the model lives: `%LOCALAPPDATA%\GondCode\speech`.
pub fn dir() -> Option<PathBuf> {
    crate::appdata::local().map(|d| d.join("speech"))
}

/// The model file, if it's downloaded.
pub fn model_path() -> Option<PathBuf> {
    dir().map(|d| d.join(FILE)).filter(|p| p.is_file())
}

/// Downloads the model to `dir`, reporting `(done, total)` bytes; a partial file is kept
/// aside until its hash checks out. `cancel` stops it.
pub fn download(
    dir: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let part = dir.join(format!("{FILE}.part"));
    let fail = |e: String| AppError::Invalid(format!("couldn't download the speech model: {e}"));
    let client = crate::install::http_client().map_err(fail)?;
    let mut resp = client
        .get(URL)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| fail(e.without_url().to_string()))?;
    let total = resp.content_length().unwrap_or(BYTES);
    let mut out = std::fs::File::create(&part)?;
    let mut hash = Sha256::new();
    let (mut done, mut buf) = (0u64, vec![0u8; 1 << 16]);
    loop {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            drop(out);
            let _ = std::fs::remove_file(&part);
            return Err(AppError::Invalid("the download was stopped".into()));
        }
        let n = resp.read(&mut buf).map_err(|e| fail(e.to_string()))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        hash.update(&buf[..n]);
        done += n as u64;
        progress(done, total);
    }
    out.flush()?;
    drop(out);
    let got: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != SHA256 {
        let _ = std::fs::remove_file(&part);
        return Err(fail("the file didn't match its checksum".into()));
    }
    let path = dir.join(FILE);
    std::fs::rename(&part, &path)?;
    Ok(path)
}

/// Whether speech to text is on (`api::speech` keeps it under the `speech` settings key).
static ON: AtomicBool = AtomicBool::new(false);

pub fn on() -> bool {
    ON.load(Ordering::SeqCst)
}

pub fn set_on(on: bool) {
    ON.store(on, Ordering::SeqCst);
}

/// The loaded model and a session on it, kept between recordings.
struct Loaded {
    _model: transcribe_cpp::Model,
    session: transcribe_cpp::Session,
}

static LOADED: Mutex<Option<Loaded>> = Mutex::new(None);

/// Text from 16 kHz mono samples in [-1, 1]. Loads the model on first use.
pub fn transcribe(pcm: &[f32]) -> AppResult<String> {
    if let Some(why) = unsupported() {
        return Err(AppError::Invalid(why));
    }
    let path = model_path().filter(|_| on()).ok_or_else(|| {
        AppError::Invalid("speech to text is off: turn it on in Settings → General".into())
    })?;
    let mut g = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        let model = transcribe_cpp::Model::load(&path)
            .map_err(|e| AppError::Invalid(format!("couldn't load the speech model: {e}")))?;
        let session = model
            .session()
            .map_err(|e| AppError::Invalid(format!("couldn't start speech to text: {e}")))?;
        *g = Some(Loaded {
            _model: model,
            session,
        });
    }
    let loaded = g.as_mut().expect("loaded above");
    let out = loaded
        .session
        .run(pcm, &transcribe_cpp::RunOptions::default())
        .map_err(|e| AppError::Invalid(format!("couldn't transcribe: {e}")))?;
    Ok(out.text.trim().to_string())
}

/// Turns it off: the model leaves memory and the disk.
pub fn remove() -> AppResult<()> {
    *LOADED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if let Some(d) = dir().filter(|d| d.exists()) {
        std::fs::remove_dir_all(d)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// On a real PC (ignored; downloads 731 MB): the real model, from download to text.
    /// GOND_SPEECH_WAV can point at a 16 kHz mono 16-bit WAV to transcribe.
    #[test]
    #[ignore]
    fn real_speech_round_trip() {
        let dir = super::dir().unwrap();
        if super::model_path().is_none() {
            let stop = std::sync::atomic::AtomicBool::new(false);
            let mut last = 0;
            super::download(&dir, &stop, |done, total| {
                let pct = done * 100 / total.max(1);
                if pct >= last + 10 {
                    last = pct;
                    eprintln!("{pct}%");
                }
            })
            .unwrap();
        }
        let pcm: Vec<f32> = match std::env::var("GOND_SPEECH_WAV") {
            Ok(p) => {
                let b = std::fs::read(p).unwrap();
                b[44..]
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                    .collect()
            }
            Err(_) => vec![0.0; 16_000],
        };
        super::set_on(true);
        let text = super::transcribe(&pcm).unwrap();
        eprintln!("heard: {text:?}");
    }
}
