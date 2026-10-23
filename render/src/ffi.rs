//! Объявления C API rlottie (`inc/rlottie_capi.h`) — только то, что нужно.

use std::os::raw::c_char;

#[repr(C)]
pub struct LottieAnimation {
    _private: [u8; 0],
}

extern "C" {
    pub fn lottie_animation_from_data(
        data: *const c_char,
        key: *const c_char,
        resource_path: *const c_char,
    ) -> *mut LottieAnimation;

    pub fn lottie_animation_destroy(animation: *mut LottieAnimation);

    pub fn lottie_animation_get_totalframe(animation: *const LottieAnimation) -> usize;

    /// Рисует кадр в буфер ARGB32 (premultiplied, порядок байт платформы).
    pub fn lottie_animation_render(
        animation: *mut LottieAnimation,
        frame_num: usize,
        buffer: *mut u32,
        width: usize,
        height: usize,
        bytes_per_line: usize,
    );
}
