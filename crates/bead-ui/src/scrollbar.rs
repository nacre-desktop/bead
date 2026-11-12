// SPDX-License-Identifier: MIT
//! Shared scrollbar geometry and host theme values.
use bead_render::{Palette, Rect};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub width: f32,
    pub hover_width: f32,
    pub hit_width: f32,
    pub inset: f32,
    pub radius: f32,
    pub minimum_thumb: f32,
    pub line_step: f32,
    pub page_overlap: f32,
    pub auto_hide_ms: f32,
    pub fade_ms: f32,
    pub alpha: f32,
    pub hover_alpha: f32,
    pub color: Value,
    pub hover_color: Value,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 6.,
            hover_width: 10.,
            hit_width: 24.,
            inset: 3.,
            radius: 3.,
            minimum_thumb: 24.,
            line_step: 24.,
            page_overlap: 24.,
            auto_hide_ms: 1000.,
            fade_ms: 160.,
            alpha: 0.7,
            hover_alpha: 1.,
            color: json!("onSurfaceVariant"),
            hover_color: json!("onSurface"),
        }
    }
}

impl Config {
    pub fn default_tokens() -> Value {
        serde_json::to_value(Self::default()).expect("scrollbar defaults serialize")
    }

    /// Resolve a color role (optionally with alpha) without tying the widget to an app palette.
    pub fn paint_color(&self, hovered: bool, palette: &Palette) -> [u8; 4] {
        let token = if hovered {
            &self.hover_color
        } else {
            &self.color
        };
        let default = palette
            .get("onSurfaceVariant")
            .unwrap_or([128, 128, 128, 255]);
        let mut color = match token {
            Value::String(role) => palette.get(role).unwrap_or(default),
            Value::Object(object) => object
                .get("role")
                .and_then(Value::as_str)
                .and_then(|role| palette.get(role).ok())
                .unwrap_or(default),
            _ => default,
        };
        let configured = if hovered {
            self.hover_alpha
        } else {
            self.alpha
        };
        let token_alpha = token["alpha"].as_f64().unwrap_or(1.) as f32;
        color[3] = (f32::from(color[3]) * configured.clamp(0., 1.) * token_alpha.clamp(0., 1.))
            .round() as u8;
        color
    }
}

pub fn validate_tokens(tokens: &Value) -> Result<(), String> {
    let object = tokens
        .as_object()
        .ok_or("theme scrollbar must be an object")?;
    for (key, value) in object {
        match key.as_str() {
            "color" | "hover_color" => {
                let valid = value.as_str().is_some_and(|s| !s.is_empty())
                    || value
                        .get("role")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty());
                if !valid {
                    return Err(format!("theme scrollbar.{key} must name a palette role"));
                }
                if value.get("alpha").is_some_and(|a| {
                    !a.as_f64()
                        .is_some_and(|n| n.is_finite() && (0. ..=1.).contains(&n))
                }) {
                    return Err(format!("theme scrollbar.{key}.alpha must be 0..1"));
                }
            }
            "auto_hide_ms" | "fade_ms" => {
                if !value
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && (0. ..=60_000.).contains(&n))
                {
                    return Err(format!("theme scrollbar.{key} must be 0..60000"));
                }
            }
            "width" | "hover_width" | "hit_width" | "inset" | "radius" | "minimum_thumb"
            | "line_step" | "page_overlap" | "alpha" | "hover_alpha" => {
                let max = if key == "alpha" || key == "hover_alpha" {
                    1.
                } else {
                    256.
                };
                if !value
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && n >= 0. && n <= max)
                {
                    return Err(format!("theme scrollbar.{key} is outside its valid range"));
                }
            }
            _ => return Err(format!("unknown theme scrollbar token {key}")),
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub track: Rect,
    pub thumb: Rect,
    pub hit: Rect,
}

/// Compute a vertical scrollbar for a viewport and its maximum scroll offset.
pub fn geometry(viewport: Rect, offset: f64, limit: f64, config: &Config) -> Option<Geometry> {
    if ![
        viewport.x,
        viewport.y,
        viewport.w,
        viewport.h,
        config.width,
        config.hit_width,
        config.inset,
        config.minimum_thumb,
    ]
    .into_iter()
    .all(f32::is_finite)
        || !offset.is_finite()
        || !limit.is_finite()
        || viewport.w <= 0.
        || viewport.h <= 0.
        || limit <= 0.
        || config.width <= 0.
        || config.hit_width <= 0.
        || config.inset < 0.
    {
        return None;
    }
    let track = Rect::new(
        viewport.x + viewport.w - config.inset - config.width,
        viewport.y + config.inset,
        config.width,
        (viewport.h - 2. * config.inset).max(0.),
    );
    if track.h <= 0. {
        return None;
    }
    let extent = f64::from(viewport.h) + limit;
    let thumb_h = (f64::from(track.h) * f64::from(viewport.h) / extent)
        .max(f64::from(config.minimum_thumb))
        .min(f64::from(track.h)) as f32;
    let travel = (track.h - thumb_h).max(0.);
    let y = track.y + travel * (offset.clamp(0., limit) / limit) as f32;
    let hit_width = config.hit_width.max(config.width).min(viewport.w);
    Some(Geometry {
        track,
        thumb: Rect::new(track.x, y, track.w, thumb_h),
        hit: Rect::new(
            viewport.x + viewport.w - hit_width,
            track.y,
            hit_width,
            track.h,
        ),
    })
}

pub fn offset_at_thumb_start(
    _viewport: Rect,
    limit: f64,
    geometry: &Geometry,
    thumb_start_y: f32,
) -> f64 {
    if !limit.is_finite() || limit <= 0. {
        return 0.;
    }
    let travel = geometry.track.h - geometry.thumb.h;
    if travel <= 0. {
        return 0.;
    }
    ((thumb_start_y - geometry.track.y) / travel).clamp(0., 1.) as f64 * limit
}

pub fn page_target(
    offset: f64,
    limit: f64,
    viewport_height: f32,
    geometry: &Geometry,
    pointer_y: f32,
) -> f64 {
    let page = f64::from(viewport_height.max(0.) * 0.9);
    let next = if pointer_y < geometry.thumb.y {
        offset - page
    } else if pointer_y >= geometry.thumb.y + geometry.thumb.h {
        offset + page
    } else {
        offset
    };
    next.clamp(0., limit.max(0.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_tracks_scroll_position_and_respects_minimum() {
        let c = Config::default();
        let v = Rect::new(0., 0., 200., 100.);
        let top = geometry(v, 0., 900., &c).unwrap();
        let middle = geometry(v, 450., 900., &c).unwrap();
        let bottom = geometry(v, 900., 900., &c).unwrap();
        assert_eq!(top.thumb.h, c.minimum_thumb);
        assert!(middle.thumb.y > top.thumb.y);
        assert!((bottom.thumb.y + bottom.thumb.h - bottom.track.y - bottom.track.h).abs() < 0.01);
        assert_eq!(offset_at_thumb_start(v, 900., &top, bottom.thumb.y), 900.);
    }

    #[test]
    fn page_click_moves_by_viewport_with_overlap() {
        let c = Config::default();
        let g = geometry(Rect::new(0., 0., 200., 100.), 0., 900., &c).unwrap();
        assert_eq!(page_target(0., 900., 100., &g, 80.), 90.);
        let bottom = geometry(Rect::new(0., 0., 200., 100.), 900., 900., &c).unwrap();
        assert_eq!(page_target(900., 900., 100., &bottom, 10.), 810.);
        assert!(geometry(Rect::new(0., 0., 200., 100.), 0., 0., &c).is_none());
    }

    #[test]
    fn virtual_list_total_count_sets_the_thumb_extent() {
        let config = Config {
            minimum_thumb: 0.,
            ..Config::default()
        };
        let viewport = Rect::new(0., 0., 200., 1000.);
        let total_extent = 1000. * 40.;
        let limit = f64::from(total_extent - viewport.h);
        let g = geometry(viewport, 0., limit, &config).unwrap();
        let expected = g.track.h * viewport.h / total_extent;
        assert!((g.thumb.h - expected).abs() < 0.01);
    }
}
