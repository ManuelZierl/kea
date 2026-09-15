mod chunk_0 {
    include!("logo_frames_0.rs");
}
mod chunk_1 {
    include!("logo_frames_1.rs");
}
mod chunk_2 {
    include!("logo_frames_2.rs");
}
mod chunk_3 {
    include!("logo_frames_3.rs");
}
mod chunk_4 {
    include!("logo_frames_4.rs");
}
mod chunk_5 {
    include!("logo_frames_5.rs");
}
mod chunk_6 {
    include!("logo_frames_6.rs");
}
mod chunk_7 {
    include!("logo_frames_7.rs");
}

pub const FRAME_COUNT: usize = 62;

pub fn path(index: usize) -> &'static str {
    match index {
        0..=7 => chunk_0::PATHS[index],
        8..=15 => chunk_1::PATHS[index - 8],
        16..=23 => chunk_2::PATHS[index - 16],
        24..=31 => chunk_3::PATHS[index - 24],
        32..=39 => chunk_4::PATHS[index - 32],
        40..=47 => chunk_5::PATHS[index - 40],
        48..=55 => chunk_6::PATHS[index - 48],
        56..=61 => chunk_7::PATHS[index - 56],
        _ => panic!("invalid Kea logo animation frame: {index}"),
    }
}
