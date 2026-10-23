// SPDX-License-Identifier: MIT OR Apache-2.0
//! JSON-lines protocol: controller to renderer on stdin, renderer to controller on stdout.
use serde::{Deserialize, Serialize};

/// What the decoder should be doing. `Release` frees the decoder (and its memory) while the
/// renderer keeps showing the last frame; `Play` after `Release` resumes from that position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decoder {
    Play,
    Pause,
    Release,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    /// Preserve aspect ratio, cover the output, centred crop.
    #[default]
    Crop,
    /// Preserve aspect ratio, contain, centre.
    Fit,
    /// Stretch to the output.
    Fill,
}

/// A wallpaper source kind. New kinds are new variants; a renderer that does not know a
/// kind fails the parse and reports an error event instead of guessing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Source {
    /// A local video file (or animated image) played without audio.
    Video {
        path: String,
        #[serde(default)]
        fit: Fit,
        #[serde(default = "yes", rename = "loop")]
        looped: bool,
    },
}
fn yes() -> bool {
    true
}
impl Source {
    pub fn path(&self) -> &str {
        match self {
            Self::Video { path, .. } => path,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Command {
    /// Replace (or with `None`, clear) the source. A new source starts from its beginning.
    Source {
        source: Option<Source>,
    },
    /// Decoder state and the outputs that should keep receiving new frames. Outputs not in
    /// `render` keep their last frame.
    State {
        decoder: Decoder,
        render: Vec<String>,
    },
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    Ready {
        pid: u32,
    },
    /// The first decoded frame of the current source is on screen on `output`.
    FirstFrame {
        output: String,
    },
    /// Decoder backend in use, for example `vaapi-copy` or `no` (software).
    Hwdec {
        mode: String,
    },
    Error {
        message: String,
    },
}

pub const MAX_LINE: usize = 16 * 1024;
pub fn encode<T: Serialize>(value: &T) -> String {
    let mut line = serde_json::to_string(value).unwrap_or_default();
    line.push('\n');
    line
}
pub fn decode<T: for<'a> Deserialize<'a>>(line: &str) -> Result<T, String> {
    if line.len() > MAX_LINE {
        return Err("line too long".into());
    }
    serde_json::from_str(line.trim()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_round_trip_and_unknown_kinds_are_rejected() {
        let command = Command::Source {
            source: Some(Source::Video {
                path: "/a b.mp4".into(),
                fit: Fit::Crop,
                looped: true,
            }),
        };
        assert_eq!(decode::<Command>(&encode(&command)).unwrap(), command);
        let state = Command::State {
            decoder: Decoder::Release,
            render: vec!["DP-1".into()],
        };
        assert_eq!(decode::<Command>(&encode(&state)).unwrap(), state);
        assert!(
            decode::<Command>(r#"{"cmd":"source","source":{"kind":"shader","path":"/x"}}"#)
                .is_err()
        );
        let defaulted: Command =
            decode(r#"{"cmd":"source","source":{"kind":"video","path":"/x.mp4"}}"#).unwrap();
        assert_eq!(
            defaulted,
            Command::Source {
                source: Some(Source::Video {
                    path: "/x.mp4".into(),
                    fit: Fit::Crop,
                    looped: true
                })
            }
        );
        assert!(decode::<Command>(&"x".repeat(MAX_LINE + 1)).is_err());
    }
}
