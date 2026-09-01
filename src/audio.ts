export interface AudioInputDeviceInfo {
  id: string;
  name: string;
  isDefault: boolean;
  channels: number;
  sampleRate: number;
}

export interface AudioInputTestResult {
  peak: number;
  rms: number;
  hasSignal: boolean;
}

export const SYSTEM_DEFAULT_MICROPHONE = "";

function deviceNameFromId(id: string): string {
  const separator = id.indexOf(":");
  const prefix = separator >= 0 ? id.slice(0, separator) : "";
  return separator >= 0 && /^\d+$/.test(prefix) ? id.slice(separator + 1) : id;
}

export function resolveMicrophoneDeviceId(
  configuredId: string | null | undefined,
  devices: AudioInputDeviceInfo[],
): string {
  if (configuredId && devices.some((device) => device.id === configuredId)) {
    return configuredId;
  }
  if (configuredId) {
    const configuredName = deviceNameFromId(configuredId);
    const sameDevice = devices.find((device) => device.name === configuredName);
    if (sameDevice) return sameDevice.id;
  }
  return SYSTEM_DEFAULT_MICROPHONE;
}
