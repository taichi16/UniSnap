#pragma once

#include <stdbool.h>
#include <stddef.h>

// Starts and stops the ScreenCaptureKit recorder inside the Tauri process.
// `error_buffer` receives a UTF-8 diagnostic on failure.
bool sck_start_recording(
    unsigned long display_index,
    double x, double y, double width, double height,
    double canvas_width, double canvas_height,
    unsigned int fps,
    bool capture_microphone,
    bool capture_system_audio,
    const char *output_path,
    char *error_buffer,
    size_t error_buffer_size
);

bool sck_stop_recording(char *error_buffer, size_t error_buffer_size);
