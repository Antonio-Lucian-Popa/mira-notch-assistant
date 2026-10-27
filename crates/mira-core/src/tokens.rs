//! Typed view over `design/tokens.json`.
//!
//! The JSON is the single source of truth for the notch's look. The Windows UI
//! reads it through these structs (the canonical file is embedded at build time
//! so the binary always matches the repo); the GNOME extension reads the same
//! JSON directly in GJS. Keep the two renderers pixel-identical by only ever
//! changing `design/tokens.json`.

use serde::Deserialize;

use crate::error::Result;

/// The canonical token file, embedded at compile time.
pub const EMBEDDED_TOKENS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../design/tokens.json"
));

#[derive(Clone, Debug, Deserialize)]
pub struct Tokens {
    pub version: u32,
    pub notch: Notch,
    pub colors: Colors,
    pub typography: Typography,
    pub motion: Motion,
    pub pages: Vec<String>,
    pub sizing: Sizing,
    pub visualizer: Visualizer,
    pub mascot: Mascot,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct NotchRect {
    pub width: f32,
    pub height: f32,
    #[serde(default, rename = "radiusTop")]
    pub radius_top: f32,
    #[serde(rename = "radiusBottom")]
    pub radius_bottom: f32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Notch {
    pub closed: NotchRect,
    pub hover: NotchRect,
    pub alert: NotchRect,
    pub minimal: NotchRect,
    #[serde(rename = "edgeGap")]
    pub edge_gap: f32,
    #[serde(rename = "contentPadding")]
    pub content_padding: f32,
    #[serde(rename = "hoverCloseDelayMs")]
    pub hover_close_delay_ms: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Colors {
    pub background: String,
    #[serde(rename = "backgroundAlert")]
    pub background_alert: String,
    pub text: String,
    #[serde(rename = "textDim")]
    pub text_dim: String,
    pub accent: String,
    #[serde(rename = "visualizerBar")]
    pub visualizer_bar: String,
    #[serde(rename = "lyricActive")]
    pub lyric_active: String,
    #[serde(rename = "lyricInactive")]
    pub lyric_inactive: String,
    #[serde(rename = "dotActive")]
    pub dot_active: String,
    #[serde(rename = "dotInactive")]
    pub dot_inactive: String,
    #[serde(rename = "batteryOk")]
    pub battery_ok: String,
    #[serde(rename = "batteryLow")]
    pub battery_low: String,
    pub charging: String,
    pub separator: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct FontToken {
    pub size: f32,
    pub weight: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Typography {
    pub clock: FontToken,
    pub title: FontToken,
    pub artist: FontToken,
    pub lyric: FontToken,
    pub alert: FontToken,
}

/// A single animation spec: either a spring or an eased tween.
#[derive(Clone, Debug, Deserialize)]
pub struct Anim {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: u32,
    #[serde(default)]
    pub easing: Option<String>,
    #[serde(default)]
    pub stiffness: Option<f32>,
    #[serde(default)]
    pub damping: Option<f32>,
    #[serde(default)]
    pub mass: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Motion {
    pub open: Anim,
    pub close: Anim,
    #[serde(rename = "alertIn")]
    pub alert_in: Anim,
    #[serde(rename = "alertOut")]
    pub alert_out: Anim,
    #[serde(rename = "pageSwap")]
    pub page_swap: Anim,
    #[serde(rename = "lyricLine")]
    pub lyric_line: Anim,
    #[serde(rename = "alertHoldMs")]
    pub alert_hold_ms: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Sizing {
    #[serde(rename = "wideMonitorMinWidthPx")]
    pub wide_monitor_min_width_px: u32,
    #[serde(rename = "wideScaleFactor")]
    pub wide_scale_factor: f32,
    #[serde(rename = "minScale")]
    pub min_scale: f32,
    #[serde(rename = "maxScale")]
    pub max_scale: f32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Visualizer {
    pub bars: u32,
    #[serde(rename = "minHeight")]
    pub min_height: f32,
    #[serde(rename = "maxHeight")]
    pub max_height: f32,
    pub gap: f32,
    pub fps: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Mascot {
    #[serde(rename = "idleTimeoutMs")]
    pub idle_timeout_ms: u32,
    #[serde(rename = "sleepTransitionMs")]
    pub sleep_transition_ms: u32,
}

impl Tokens {
    /// Parse the embedded canonical token file. Infallible in practice (the
    /// file is validated by a unit test), but returns `Result` for symmetry.
    pub fn embedded() -> Result<Self> {
        Self::from_json_str(EMBEDDED_TOKENS_JSON)
    }

    pub fn from_json_str(s: &str) -> Result<Self> {
        Ok(serde_json::from_str(s)?)
    }

    pub fn from_path(path: &std::path::Path) -> Result<Self> {
        Self::from_json_str(&std::fs::read_to_string(path)?)
    }

    /// Size multiplier for a monitor `monitor_width_px` wide, before the user's
    /// manual `size_scale`. Wide monitors get a larger notch, per the design.
    pub fn auto_scale_for(&self, monitor_width_px: u32) -> f32 {
        if monitor_width_px >= self.sizing.wide_monitor_min_width_px {
            self.sizing.wide_scale_factor
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_tokens_parse() {
        let t = Tokens::embedded().expect("design/tokens.json must parse");
        assert_eq!(t.version, 1);
        assert_eq!(t.pages.len(), 12);
        assert!(t.pages.iter().any(|page| page == "reminders"));
        assert_eq!(t.pages[0], "music");
        assert_eq!(t.pages[1], "lyrics");
        assert!(t.notch.hover.width > t.notch.closed.width);
        assert!(t.notch.closed.radius_bottom > 0.0);
    }

    #[test]
    fn colors_are_hex_strings() {
        let t = Tokens::embedded().unwrap();
        assert!(t.colors.background.starts_with('#'));
        assert!(t.colors.lyric_active.starts_with('#'));
    }

    #[test]
    fn open_motion_is_spring() {
        let t = Tokens::embedded().unwrap();
        assert_eq!(t.motion.open.kind, "spring");
        assert!(t.motion.open.stiffness.is_some());
    }

    #[test]
    fn wide_monitor_scales_up() {
        let t = Tokens::embedded().unwrap();
        let narrow = t.auto_scale_for(1920);
        let wide = t.auto_scale_for(3840);
        assert_eq!(narrow, 1.0);
        assert!(wide > narrow);
    }
}
