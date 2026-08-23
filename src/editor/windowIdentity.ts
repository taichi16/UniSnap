export function getCaptureMonitorIndex(label: string): number {
  const match = label.match(/capture_(\d+)_/);
  return match ? parseInt(match[1], 10) : 0;
}
