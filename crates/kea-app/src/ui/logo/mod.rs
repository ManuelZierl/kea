//! Reusable Kea logo presentation.

mod frames;

use self::frames as logo_frames;
use gpui::prelude::*;
use gpui::{
    img, px, Animation, AnimationExt, App, ElementId, Hsla, Image, ImageFormat, IntoElement,
    Pixels, RenderOnce, Window,
};
use gpui_component::ActiveTheme;
use std::{sync::Arc, time::Duration};

/// Length of one complete "peck at the ground" logo cycle.
///
/// The supplied animation is 124 frames at 24 fps. The component stores 62
/// vector poses sampled from every second video frame, without embedding a
/// video or raster animation. Counters are opened up relative to the source
/// (larger eye hole, wider beak gap) so the bird stays legible at small sizes
/// such as the composer prefix. Playback runs 20% faster than the source plus
/// another 10% (effective ~33 fps) so the small bird feels responsive.
pub const KEA_LOGO_PECK_DURATION: Duration = Duration::from_millis(3_721);

/// Number of vector poses in one peck cycle.
pub const FRAME_COUNT: usize = logo_frames::FRAME_COUNT;

/// Decode one animation frame for `color` without displaying it.
///
/// GPUI paints nothing for an SVG frame until its asset is decoded, which made
/// the first typed peck flicker until every frame had been seen once. Warming
/// all frames after launch (a few pump ticks at a time) keeps that decode work
/// out of the typing path.
pub fn frame_image(frame: usize, color: Hsla) -> Arc<Image> {
    Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        frame_svg(logo_frames::path(frame), color),
    ))
}

/// Decode one frame into the asset cache; the next paint of that frame is free.
pub fn warm_frame(frame: usize, color: Hsla, window: &mut Window, cx: &mut App) {
    let _ = frame_image(frame, color).get_render_image(window, cx);
}

/// Reusable animated Kea logo.
///
/// The animation is stored as monochrome SVG path frames. This keeps the
/// background transparent, allows arbitrary display sizes, and lets the logo
/// follow the active theme foreground color.
///
/// The first and last frames are deliberately identical to the normal Kea
/// logo. A component plays one peck by default and then remains on the normal
/// logo; call [`Self::repeat`] when a continuously looping animation is wanted.
///
/// Give simultaneously visible instances distinct element IDs so GPUI can
/// track their animation state independently.
///
/// # Example
///
/// ```ignore
/// use gpui::px;
/// use kea_app::ui::logo::KeaLogoAnimation;
///
/// let logo = KeaLogoAnimation::new("terminal-logo").size(px(96.));
/// ```
#[derive(IntoElement)]
pub struct KeaLogoAnimation {
    id: ElementId,
    size: Pixels,
    color: Option<Hsla>,
    repeat: bool,
}

impl KeaLogoAnimation {
    /// Create a one-shot animated logo with a default size of 96 px.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            size: px(96.),
            color: None,
            repeat: false,
        }
    }

    /// Set both width and height of the square logo surface.
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Override the theme foreground color while keeping the animation monochrome.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Repeat the peck animation continuously instead of stopping after one cycle.
    pub fn repeat(mut self) -> Self {
        self.repeat = true;
        self
    }
}

/// Static Kea logo: the resting bird (animation frame 0) without any animation.
///
/// Use this wherever the logo is idle; switch to [`KeaLogoAnimation`] for a
/// complete peck cycle. Both render the identical resting bird, so swapping
/// between them at a cycle boundary is seamless.
#[derive(IntoElement)]
pub struct KeaLogo {
    size: Pixels,
    color: Option<Hsla>,
}

impl KeaLogo {
    /// Create a static logo with a default size of 96 px.
    pub fn new() -> Self {
        Self {
            size: px(96.),
            color: None,
        }
    }

    /// Set both width and height of the square logo surface.
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Override the theme foreground color while keeping the logo monochrome.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl Default for KeaLogo {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for KeaLogo {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        KeaLogoFrame {
            size: self.size,
            frame: 0,
            color: self.color.unwrap_or(cx.theme().foreground),
        }
    }
}

#[derive(IntoElement)]
struct KeaLogoFrame {
    size: Pixels,
    frame: usize,
    color: Hsla,
}
impl RenderOnce for KeaLogoFrame {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let image = frame_image(self.frame, self.color);
        img(image).size(self.size)
    }
}

impl RenderOnce for KeaLogoAnimation {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = self.color.unwrap_or(cx.theme().foreground);
        let animation = Animation::new(KEA_LOGO_PECK_DURATION);
        let animation = if self.repeat {
            animation.repeat()
        } else {
            animation
        };

        KeaLogoFrame {
            size: self.size,
            frame: 0,
            color,
        }
        .with_animation(self.id, animation, |mut logo, progress| {
            logo.frame = frame_index(progress);
            logo
        })
    }
}

fn frame_index(progress: f32) -> usize {
    let progress = progress.clamp(0.0, 1.0);
    ((progress * logo_frames::FRAME_COUNT as f32).floor() as usize)
        .min(logo_frames::FRAME_COUNT - 1)
}

fn frame_svg(path: &str, color: Hsla) -> Vec<u8> {
    let color = color.to_rgb();
    let r = channel_byte(color.r);
    let g = channel_byte(color.g);
    let b = channel_byte(color.b);
    let alpha = color.a.clamp(0.0, 1.0);

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="137 132 520 520"><path fill="#{r:02x}{g:02x}{b:02x}" fill-opacity="{alpha:.4}" fill-rule="evenodd" d="{path}"/></svg>"##
    )
    .into_bytes()
}

fn channel_byte(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/logo/mod.rs"]
mod tests;
