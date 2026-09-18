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
    const { assert!(logo_frames::FRAME_COUNT > 2) };
    assert!((1..logo_frames::FRAME_COUNT - 1)
        .map(logo_frames::path)
        .any(|frame| frame != logo_frames::path(0)));
}

#[test]
fn frame_images_are_stable_cache_keys() {
    // Warming works only because the painted element builds the identical
    // bytes (and therefore asset id) for the same frame and color.
    let color = Hsla::black();
    assert_eq!(frame_image(0, color).id(), frame_image(0, color).id());
    assert_ne!(frame_image(0, color).id(), frame_image(1, color).id());
    assert_ne!(
        frame_image(0, color).id(),
        frame_image(0, Hsla::white()).id()
    );
}
