use tauri::Emitter;
use crate::scroll_matching::{find_scroll_shift, find_scroll_shift_near, frames_are_stable};
use crate::scroll_masks::{fixed_column_mask, fixed_row_mask};
use crate::scroll_target::{classify_scroll_target, ScrollCaptureStrategy};
use crate::scroll_input::ScrollController;
use crate::scroll_composite::compose_scroll_frames;
use crate::scroll_target_window::resolve_scroll_target;
use crate::capture_output::encode_png_data_url;
use crate::scroll_capture_helpers::{crop_selection, strategy_label};
use crate::scroll_metrics::ScrollMetrics;
use crate::scroll_session::ScrollCaptureSession;

use std::sync::atomic::{AtomicBool, Ordering};

static SCROLL_CANCELLED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn cancel_scroll_capture() {
    SCROLL_CANCELLED.store(true, Ordering::SeqCst);
}

const MAX_SCROLL_STEPS: usize = 120;
const MAX_STITCHED_HEIGHT: u32 = 60_000;

#[tauri::command]
pub fn auto_scroll_capture_window(
    app: tauri::AppHandle,
    monitor_index: usize,
    selection_x: f64,
    selection_y: f64,
    selection_width: f64,
    selection_height: f64,
) -> Result<String, String> {
    SCROLL_CANCELLED.store(false, Ordering::SeqCst);

    let target = resolve_scroll_target(
        &app, monitor_index, selection_x, selection_y, selection_width, selection_height,
    )?;
    let coordinate_scale = target.coordinate_scale;
    let global_selection_x = target.global_x;
    let global_selection_y = target.global_y;
    let global_selection_width = target.width;
    let global_selection_height = target.height;
    let win = target.window;
    let target_app = win.app_name().unwrap_or_default();
    let target_title = win.title().unwrap_or_default();
    let strategy = classify_scroll_target(&target_app, &target_title);
    let strategy_name = strategy_label(strategy);
    eprintln!(
        "[scroll] target app={:?} title={:?} strategy={}",
        target_app, target_title, strategy_name
    );
    if strategy != ScrollCaptureStrategy::DesktopStitch {
        eprintln!(
            "[scroll] fallback=desktop-stitch reason=content-layer-export-not-available"
        );
        let message = match strategy {
            ScrollCaptureStrategy::BrowserPage => {
                "已辨識為瀏覽器；目前使用桌面拼接，無法保證固定網頁元件後方內容完整"
            }
            ScrollCaptureStrategy::DocumentApp => {
                "已辨識為文件應用程式；目前使用桌面拼接，建議優先使用文件匯出或列印功能"
            }
            ScrollCaptureStrategy::DesktopStitch => "",
        };
        let _ = app.emit(
            "scroll-capture-strategy",
            serde_json::json!({ "strategy": strategy_name, "message": message }),
        );
    }

    let win_x = win.x().map_err(|e| e.to_string())?;
    let win_y = win.y().map_err(|e| e.to_string())?;
    let crop_x = global_selection_x.saturating_sub(win_x) as u32;
    let crop_y = global_selection_y.saturating_sub(win_y) as u32;
    eprintln!(
        "[scroll] selection logical=({},{} {}x{}) scale={:.3} global=({},{} {}x{}) window_origin=({},{}), crop=({},{} {}x{})",
        selection_x,
        selection_y,
        selection_width,
        selection_height,
        coordinate_scale,
        global_selection_x,
        global_selection_y,
        global_selection_width,
        global_selection_height,
        win_x,
        win_y,
        crop_x,
        crop_y,
        global_selection_width,
        global_selection_height
    );

    let mut input = ScrollController::new()?;
    // Move the pointer over the selected content without synthesizing a
    // click.  Clicking here can activate a browser tab or press a control
    // before scrolling starts.
    input.position_pointer(global_selection_x, global_selection_y)?;
    std::thread::sleep(std::time::Duration::from_millis(250));

    let first_frame = crop_selection(
        &win.capture_image().map_err(|e| format!("Capture frame 1 failed: {}", e))?,
        crop_x,
        crop_y,
        global_selection_width,
        global_selection_height,
    )?;
    let frame_width = first_frame.width();
    let frame_height = first_frame.height();
    let mut previous_frame = first_frame.clone();
    let mut session = ScrollCaptureSession::new(first_frame);
    let mut metrics = ScrollMetrics::new();
    metrics.final_height = frame_height;
    let mut stable_attempts = 0usize;
    let mut reached_bottom = false;
    let mut was_cancelled = false;

    for step in 1..=MAX_SCROLL_STEPS {
        metrics.capture_steps += 1;
        if SCROLL_CANCELLED.load(Ordering::SeqCst) {
            was_cancelled = true;
            break;
        }

        input
            .scroll_down(6)
            .map_err(|e| format!("Failed to scroll target window at step {}: {}", step, e))?;
        std::thread::sleep(std::time::Duration::from_millis(350));

        if SCROLL_CANCELLED.load(Ordering::SeqCst) {
            was_cancelled = true;
            break;
        }

        let mut next_frame = crop_selection(
            &win.capture_image().map_err(|e| format!("Capture frame {} failed: {}", step + 1, e))?,
            crop_x,
            crop_y,
            global_selection_width,
            global_selection_height,
        )?;
        let mut settle_attempts = 0usize;
        // Do not stitch a frame while the target is still animating or
        // re-laying out lazy content.  Capture successive samples until two
        // adjacent samples are visually stable, up to a bounded timeout.
        for attempt in 1..=8 {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let candidate = crop_selection(
                &win.capture_image().map_err(|e| format!("Capture settle frame {} failed: {}", step + 1, e))?,
                crop_x,
                crop_y,
                global_selection_width,
                global_selection_height,
            )?;
            settle_attempts = attempt;
            if frames_are_stable(&next_frame, &candidate) {
                next_frame = candidate;
                break;
            }
            next_frame = candidate;
        }
        metrics.settle_attempts += settle_attempts;
        eprintln!("[scroll] step={} settle_attempts={}", step, settle_attempts);
        if next_frame.dimensions() != previous_frame.dimensions() {
            return Err("Target window size changed during long capture".to_string());
        }

        if SCROLL_CANCELLED.load(Ordering::SeqCst) {
            was_cancelled = true;
            break;
        }

        let detected_shift = if session.frame_count() > 1 {
            let offsets = session.offsets();
            let expected = offsets[1].saturating_sub(offsets[0]);
            find_scroll_shift_near(&previous_frame, &next_frame, expected)
        } else {
            find_scroll_shift(&previous_frame, &next_frame)
        };
        if let Some(shift) = detected_shift {
            stable_attempts = 0;
            let current_offset = session.total_height().saturating_sub(frame_height);
            let next_offset = current_offset
                .checked_add(shift)
                .ok_or_else(|| "Long screenshot height overflow".to_string())?;
            let fixed_mask = (
                fixed_row_mask(&previous_frame, &next_frame),
                fixed_column_mask(&previous_frame, &next_frame),
            );
            session.append(next_frame.clone(), next_offset, fixed_mask, MAX_STITCHED_HEIGHT)
                .map_err(|error| format!("{error}，請縮小範圍或分段擷取"))?;
            metrics.accepted_frames += 1;
            metrics.final_height = session.total_height();
            eprintln!("[scroll] step={} shift={} offset={} frame={}x{}", step, shift, next_offset, frame_width, frame_height);
            previous_frame = next_frame;
        } else {
            if frames_are_stable(&previous_frame, &next_frame) {
                stable_attempts += 1;
                if stable_attempts >= 2 {
                    reached_bottom = true;
                    break;
                }
            } else {
                if SCROLL_CANCELLED.load(Ordering::SeqCst) {
                    was_cancelled = true;
                    break;
                }
                // Some windows do not expose a detectable scrollable region
                // (or their content changes without a stable pixel shift).
                // Keep the initial frame instead of reporting a failed long
                // screenshot; the user still receives the selected window.
                metrics.rejected_frames += 1;
                if session.frame_count() == 1 {
                    eprintln!("{}", metrics.summary("single-frame-fallback"));
                    return encode_png_data_url(session.frames().first().expect("initial frame"));
                }
                metrics.alignment_failures += 1;
                eprintln!("{}", metrics.summary("alignment-failed"));
                return Err(format!(
                    "第 {} 次捲動後無法可靠比對影像；已停止以避免產生缺段截圖",
                    step
                ));
            }
        }
    }

    if !reached_bottom && !was_cancelled {
        eprintln!("{}", metrics.summary("safety-limit"));
        return Err(format!(
            "已達 {} 次安全上限但尚未確認頁尾，未輸出可能不完整的截圖",
            MAX_SCROLL_STEPS
        ));
    }

    let outcome = if was_cancelled { "cancelled" } else { "saved" };
    eprintln!("{}", metrics.summary(outcome));
    compose_scroll_frames(session.frames(), session.offsets(), session.fixed_masks(), session.total_height())
}

#[cfg(test)]
mod capture_tests {
    use super::{
        classify_scroll_target, find_scroll_shift, find_scroll_shift_near, fixed_column_mask,
        fixed_row_mask, frames_are_stable,
        ScrollCaptureStrategy,
    };
    use crate::capture_geometry::{work_area_crop_bounds, CropBounds};
    use image::{imageops::crop_imm, Rgba, RgbaImage};

    fn patterned_document(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            Rgba([
                ((x * 17 + y * 3) % 251) as u8,
                ((x * 5 + y * 11) % 247) as u8,
                ((x * 13 + y * 7) % 241) as u8,
                255,
            ])
        })
    }

    #[test]
    fn detects_known_vertical_scroll_shift() {
        let document = patterned_document(320, 900);
        let before = crop_imm(&document, 0, 100, 320, 360).to_image();
        let after = crop_imm(&document, 0, 237, 320, 360).to_image();

        assert_eq!(find_scroll_shift(&before, &after), Some(137));
        assert_eq!(find_scroll_shift_near(&before, &after, 137), Some(137));
    }

    #[test]
    fn identical_frames_are_treated_as_stopped() {
        let frame = patterned_document(320, 360);

        assert!(frames_are_stable(&frame, &frame));
        assert_eq!(find_scroll_shift(&frame, &frame), None);
    }

    #[test]
    fn rejects_frames_with_different_dimensions() {
        let before = patterned_document(320, 360);
        let after = patterned_document(300, 360);

        assert_eq!(find_scroll_shift(&before, &after), None);
    }

    #[test]
    fn identifies_fixed_sidebar_without_marking_document_columns() {
        let document = patterned_document(160, 320);
        let mut before = crop_imm(&document, 0, 0, 160, 240).to_image();
        let mut after = crop_imm(&document, 0, 24, 160, 240).to_image();
        for y in 0..240 {
            for x in 0..32 {
                let pixel = if ((x / 8) + (y / 8)) % 2 == 0 {
                    Rgba([20, 40, 80, 255])
                } else {
                    Rgba([220, 180, 60, 255])
                };
                before.put_pixel(x, y, pixel);
                after.put_pixel(x, y, pixel);
            }
        }
        let mask = fixed_column_mask(&before, &after);
        assert!(mask[..32].iter().all(|value| *value));
        assert!(mask[48..].iter().any(|value| !*value));
    }

    #[test]
    fn identifies_fixed_bottom_band_only_when_contiguous() {
        let document = patterned_document(160, 320);
        let mut before = crop_imm(&document, 0, 0, 160, 240).to_image();
        let mut after = crop_imm(&document, 0, 24, 160, 240).to_image();
        for y in 208..240 {
            for x in 0..160 {
                let pixel = if ((x / 8) + (y / 8)) % 2 == 0 {
                    Rgba([32, 32, 32, 255])
                } else {
                    Rgba([210, 210, 210, 255])
                };
                before.put_pixel(x, y, pixel);
                after.put_pixel(x, y, pixel);
            }
        }
        let mask = fixed_row_mask(&before, &after);
        assert!(mask[208..].iter().all(|value| *value));
        assert!(mask[..192].iter().any(|value| !*value));
    }

    #[test]
    fn classifies_browser_and_document_targets_before_generic_stitching() {
        assert_eq!(
            classify_scroll_target("Google Chrome", "Example"),
            ScrollCaptureStrategy::BrowserPage
        );
        assert_eq!(
            classify_scroll_target("Microsoft Word", "Document1"),
            ScrollCaptureStrategy::DocumentApp
        );
        assert_eq!(
            classify_scroll_target("Preview", "Screenshot"),
            ScrollCaptureStrategy::DocumentApp
        );
        assert_eq!(
            classify_scroll_target("Terminal", "zsh"),
            ScrollCaptureStrategy::DesktopStitch
        );
    }

    #[test]
    fn work_area_on_offset_monitor_is_converted_to_local_image_coordinates() {
        // Secondary monitor begins at desktop x=2408, while its usable area
        // begins 40 physical pixels below its own top edge (menu bar).
        assert_eq!(
            work_area_crop_bounds(2408, 0, 2408, 40, 1920, 1040, 1920, 1080),
            Some(CropBounds {
                x: 0,
                y: 40,
                width: 1920,
                height: 1040,
            })
        );
    }

    #[test]
    fn work_area_is_not_scaled_a_second_time_on_retina_monitor() {
        assert_eq!(
            work_area_crop_bounds(0, 0, 0, 72, 2408, 1434, 2408, 1506),
            Some(CropBounds {
                x: 0,
                y: 72,
                width: 2408,
                height: 1434,
            })
        );
    }
}

// Fix 9: Frontend handles its own show/hide for full/work area captures.
// Backend only captures and saves; no restore needed here.
