// The engine's C entry points for call video and the camera (packages/matrix-ffi/src/native.rs).
// The symbols live in the engine's static library, which the matrix-native pod links.

#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef void (*bm_draw_fn)(void *context, uint32_t width, uint32_t height, const uint8_t *rgba, size_t length);

uint64_t bm_video_attach(uint64_t video, bool contain, void *context, bm_draw_fn draw);
void bm_video_resize(uint64_t token, uint32_t width, uint32_t height);
void bm_video_detach(uint64_t token);

bool bm_camera_frame_nv12(uint32_t width, uint32_t height, uint32_t rotation, const uint8_t *y, uint32_t stride_y,
                          const uint8_t *uv, uint32_t stride_uv);
void bm_camera_lost(void);
