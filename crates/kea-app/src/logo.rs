//! Reusable Kea logo presentation.

mod logo_frames;

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
/// video or raster animation.
pub const KEA_LOGO_PECK_DURATION: Duration = Duration::from_millis(5_167);

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
/// use kea_app::logo::KeaLogoAnimation;
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

#[derive(IntoElement)]
struct KeaLogoFrame {
    size: Pixels,
    frame: usize,
    color: Hsla,
}

impl RenderOnce for KeaLogoFrame {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let image = Image::from_bytes(
            ImageFormat::Svg,
            frame_svg(logo_frames::path(self.frame), self.color),
        );
        img(Arc::new(image)).size(self.size)
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
mod tests {
    use super::*;

    #[test]
    fn animation_starts_and_ends_on_the_same_logo() {
        assert_eq!(frame_index(0.0), 0);
        assert_eq!(frame_index(1.0), logo_frames::FRAME_COUNT - 1);
        assert_eq!(
            logo_frames::path(0),
            logo_frames::path(logo_frames::FRAME_COUNT - 1),
            "the animation must settle exactly back to the normal logo"
        );
    }

    #[test]
    fn frame_svg_is_tintable_and_transparent() {
        let svg = String::from_utf8(frame_svg(logo_frames::path(0), Hsla::black())).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains(r##"fill="#000000""##));
        assert!(!svg.contains("<rect"));
    }

    #[test]
    fn animation_contains_motion() {
        assert!(logo_frames::FRAME_COUNT > 2);
        assert!((1..logo_frames::FRAME_COUNT - 1)
            .map(logo_frames::path)
            .any(|frame| frame != logo_frames::path(0)));
    }
}
