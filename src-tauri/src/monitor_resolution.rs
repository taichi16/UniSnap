use xcap::Monitor;

/// Resolve the xcap monitor corresponding to a Tauri monitor.
///
/// Coordinates are preferred because monitor ordering can differ between
/// Tauri and xcap. Name and index are conservative fallbacks for displays
/// that do not expose stable coordinates.
pub fn resolve_xcap_monitor<'a>(
    tauri_monitor: &tauri::Monitor,
    xcap_monitors: &'a [Monitor],
    fallback_index: usize,
) -> Option<&'a Monitor> {
    let phys_x = tauri_monitor.position().x;
    let phys_y = tauri_monitor.position().y;
    xcap_monitors
        .iter()
        .find(|monitor| {
            monitor.x().unwrap_or(i32::MIN) == phys_x && monitor.y().unwrap_or(i32::MIN) == phys_y
        })
        .or_else(|| {
            let target_name = tauri_monitor.name().cloned().unwrap_or_default();
            xcap_monitors
                .iter()
                .find(|monitor| monitor.name().unwrap_or_default() == target_name)
        })
        .or_else(|| xcap_monitors.get(fallback_index))
}
