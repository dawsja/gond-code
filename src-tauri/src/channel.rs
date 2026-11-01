//! Dev and prod: two separate apps on one PC, so you can work on Gond Code inside Gond Code.
//!
//! A debug build (`bun run tauri dev`, `cargo run`, `tauri build --debug`) is **Gond Code Dev**;
//! a release build is **Gond Code**. `--features dev` makes an optimised build Dev too. The
//! choice is made at compile time, so a release build can never touch Dev's data and Dev can
//! never touch yours.
//!
//! What differs, all from here:
//! - the bundle identifier (`app.gond.code.dev`): its own single-instance lock, AppUserModelID
//!   (taskbar group), WebView2 profile and saved window position;
//! - the data folders (`%APPDATA%\GondCodeDev`, `%LOCALAPPDATA%\GondCodeDev`): its own database,
//!   sessions, saved screens, speech model;
//! - the name in the title bar, tray and status bar ("Gond Code Dev"), and an amber dot on the
//!   window and tray icon;
//! - no self-update: Dev is updated by pulling and building again.

use tauri::image::Image;
use tauri::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Prod,
    Dev,
}

/// This build's channel.
pub const CHANNEL: Channel = if cfg!(any(debug_assertions, feature = "dev")) {
    Channel::Dev
} else {
    Channel::Prod
};

/// Whether this build is Gond Code Dev.
pub const fn is_dev() -> bool {
    matches!(CHANNEL, Channel::Dev)
}

impl Channel {
    /// The bundle identifier: prod's is tauri.conf.json's, unchanged since the first release.
    pub const fn identifier(self) -> &'static str {
        match self {
            Channel::Prod => "app.gond.code",
            Channel::Dev => "app.gond.code.dev",
        }
    }

    /// The name people see: title bar, tray, status bar.
    pub const fn name(self) -> &'static str {
        match self {
            Channel::Prod => "Gond Code",
            Channel::Dev => "Gond Code Dev",
        }
    }

    /// The folder under the OS data dirs.
    pub const fn data_dir(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Channel::Prod, true) => "GondCode",
            (Channel::Prod, false) => "gond-code",
            (Channel::Dev, true) => "GondCodeDev",
            (Channel::Dev, false) => "gond-code-dev",
        }
    }

    /// As the UI spells it (`/update.channel`).
    pub const fn id(self) -> &'static str {
        match self {
            Channel::Prod => "prod",
            Channel::Dev => "dev",
        }
    }
}

/// Points the app's config at this channel before the app is built: the identifier (which the
/// single-instance lock, window state and WebView2 profile key on), the product name and the
/// windows' titles. Prod's config is left as tauri.conf.json has it.
pub fn apply(config: &mut Config, channel: Channel) {
    if channel == Channel::Prod {
        return;
    }
    config.identifier = channel.identifier().into();
    config.product_name = Some(channel.name().into());
    for w in &mut config.app.windows {
        w.title = channel.name().into();
    }
}

/// `icon` with Dev's mark: an amber dot, ringed in black, in the bottom-right corner, so the two
/// apps are told apart in the taskbar and the tray at a glance.
pub fn badge(icon: &Image<'_>) -> Image<'static> {
    let (w, h) = (icon.width(), icon.height());
    let mut rgba = icon.rgba().to_vec();
    paint_dot(&mut rgba, w, h);
    Image::new_owned(rgba, w, h)
}

const AMBER: [u8; 3] = [0xF5, 0x9E, 0x0B];

fn paint_dot(rgba: &mut [u8], w: u32, h: u32) {
    let size = w.min(h) as f32;
    let r = size * 0.22;
    let ring = (size * 0.06).max(1.0);
    let (cx, cy) = (w as f32 - r - ring, h as f32 - r - ring);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let colour = if d <= r {
                AMBER
            } else if d <= r + ring {
                [0, 0, 0]
            } else {
                continue;
            };
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&colour);
            rgba[i + 3] = 0xFF;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_and_prod_never_share_identity_or_data() {
        assert_ne!(Channel::Dev.identifier(), Channel::Prod.identifier());
        assert_ne!(Channel::Dev.data_dir(), Channel::Prod.data_dir());
        assert_ne!(Channel::Dev.name(), Channel::Prod.name());
        // Prod keeps the shipped identity: changing it would orphan users' data and updates.
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], Channel::Prod.identifier());
        assert_eq!(conf["productName"], Channel::Prod.name());
    }

    #[test]
    #[cfg(any(debug_assertions, feature = "dev"))]
    fn tests_build_as_dev() {
        // `cargo test` is a debug build: it is Dev, so nothing it does reaches prod's data.
        assert!(is_dev());
        assert_eq!(crate::appdata::DIR, Channel::Dev.data_dir());
    }

    #[test]
    fn apply_renames_dev_and_leaves_prod_alone() {
        let base: Config = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut prod = base.clone();
        apply(&mut prod, Channel::Prod);
        assert_eq!(prod.identifier, "app.gond.code");
        assert_eq!(prod.app.windows[0].title, "Gond Code");

        let mut dev = base;
        apply(&mut dev, Channel::Dev);
        assert_eq!(dev.identifier, "app.gond.code.dev");
        assert_eq!(dev.product_name.as_deref(), Some("Gond Code Dev"));
        assert_eq!(dev.app.windows[0].title, "Gond Code Dev");
    }

    #[test]
    fn badge_paints_the_corner_only() {
        let (w, h) = (32, 32);
        let mut rgba = vec![0x10; (w * h * 4) as usize];
        paint_dot(&mut rgba, w, h);
        let px = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [rgba[i], rgba[i + 1], rgba[i + 2]]
        };
        assert_eq!(px(0, 0), [0x10; 3]);
        assert_eq!(px(w - 9, h - 9), AMBER);
    }
}
