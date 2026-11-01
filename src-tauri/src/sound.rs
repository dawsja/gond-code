//! The sound cues: a chime when an agent is done, an alert when it needs you. Both are made here
//! (a few sine notes with a soft attack and a bell's decay, as a WAV in memory), so the app ships
//! no audio files, and played one after another from a queue: two agents finishing together play
//! two chimes, not one cut off by the other. A cue already waiting isn't queued twice. On
//! Windows it's `PlaySoundW` from memory, on a thread of the app's own: no process per sound.

use serde::Deserialize;

/// Which cue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Cue {
    /// An agent stopped working while you were elsewhere.
    Done,
    /// An agent is asking you something.
    Needs,
}

const RATE: u32 = 22_050;

/// One note: its pitch (Hz), when it starts and how long it rings (seconds), how loud (0–1).
struct Note(f32, f32, f32, f32);

fn notes(cue: Cue) -> &'static [Note] {
    match cue {
        // Rising, settled: E5 then A5.
        Cue::Done => &[Note(659.25, 0.0, 0.45, 0.30), Note(880.0, 0.11, 0.6, 0.30)],
        // Two quick, brighter pings on B5: something wants an answer.
        Cue::Needs => &[
            Note(987.77, 0.0, 0.22, 0.32),
            Note(987.77, 0.16, 0.32, 0.32),
        ],
    }
}

/// The cue as a 16-bit mono WAV file.
pub fn wav(cue: Cue) -> Vec<u8> {
    let ns = notes(cue);
    let secs = ns.iter().map(|n| n.1 + n.2).fold(0.0, f32::max);
    let len = (secs * RATE as f32) as usize;
    let mut pcm = vec![0f32; len];
    for Note(hz, at, ring, gain) in ns {
        let start = (at * RATE as f32) as usize;
        let n = ((ring * RATE as f32) as usize).min(len - start);
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            // 4 ms attack (no click), then a bell's exponential decay to silence by the end.
            let attack = (t / 0.004).min(1.0);
            let decay = (-t * 6.0 / ring).exp();
            let ph = std::f32::consts::TAU * hz * t;
            // A touch of the octave above makes it a chime rather than a beep.
            let tone = ph.sin() + 0.18 * (2.0 * ph).sin();
            pcm[start + i] += gain * attack * decay * tone / 1.18;
        }
    }
    let data: Vec<u8> = pcm
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes())
        .collect();
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes a second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits a sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Queues a cue. False when the same cue is already waiting (it plays once for both).
pub fn play(cue: Cue) -> bool {
    queue::push(cue)
}

mod queue {
    use super::Cue;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{sync_channel, SyncSender};
    use std::sync::OnceLock;

    static WAITING: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];
    static TX: OnceLock<SyncSender<Cue>> = OnceLock::new();

    fn slot(c: Cue) -> &'static AtomicBool {
        &WAITING[match c {
            Cue::Done => 0,
            Cue::Needs => 1,
        }]
    }

    pub fn push(cue: Cue) -> bool {
        if slot(cue).swap(true, Ordering::AcqRel) {
            return false;
        }
        let tx = TX.get_or_init(|| {
            let (tx, rx) = sync_channel::<Cue>(4);
            let sounds = [super::wav(Cue::Done), super::wav(Cue::Needs)];
            let _ = std::thread::Builder::new()
                .name("sound".into())
                .spawn(move || {
                    for c in rx {
                        slot(c).store(false, Ordering::Release);
                        let _ = super::out(&sounds[matches!(c, Cue::Needs) as usize]);
                        // A breath between cues, so two read as two.
                        std::thread::sleep(std::time::Duration::from_millis(120));
                    }
                });
            tx
        });
        if tx.try_send(cue).is_err() {
            slot(cue).store(false, Ordering::Release);
            return false;
        }
        true
    }
}

/// Plays a WAV to the end (on the queue's thread). False if Windows couldn't (no audio device).
#[cfg(windows)]
fn out(wav: &[u8]) -> bool {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC};
    // SAFETY: with SND_MEMORY the first argument is the WAV's bytes, which outlive the call.
    unsafe {
        PlaySoundW(
            wav.as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_SYNC | SND_NODEFAULT,
        ) != 0
    }
}

#[cfg(not(windows))]
fn out(_wav: &[u8]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_are_short_clean_wavs() {
        for cue in [Cue::Done, Cue::Needs] {
            let w = wav(cue);
            assert_eq!(&w[..4], b"RIFF");
            assert_eq!(&w[8..16], b"WAVEfmt ");
            let data = u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize;
            assert_eq!(w.len(), 44 + data);
            let samples: Vec<i16> = w[44..]
                .chunks(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect();
            let secs = samples.len() as f32 / RATE as f32;
            assert!((0.3..1.0).contains(&secs), "{cue:?} lasts {secs}s");
            // Never clipped, and it fades out rather than stopping with a click.
            assert!(samples.iter().all(|s| s.unsigned_abs() < i16::MAX as u16));
            assert!(samples.last().unwrap().unsigned_abs() < 200);
        }
    }

    /// Plays both cues out loud: `cargo test --lib sound -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "plays sound"]
    fn windows_plays_the_cues() {
        assert!(out(&wav(Cue::Done)));
        assert!(out(&wav(Cue::Needs)));
    }
}
