export function getCaptureMonitorIndex(label: string): number {
  const match = label.match(/capture_(?:record_|screenshot_|scroll_)?(\d+)_/);
  return match ? parseInt(match[1], 10) : 0;
}
