import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { resolveMicrophoneDeviceId, type AudioInputDeviceInfo } from "../audio";

interface StoredMicrophoneConfig {
  microphone_device_id?: string | null;
}

export function useRecordingPreferences(enabled: boolean) {
  const [recordAudio, setRecordAudio] = useState(false);
  const [recordSystemAudio, setRecordSystemAudio] = useState(false);
  const [recordingFps, setRecordingFps] = useState(30);
  const [audioInputDevices, setAudioInputDevices] = useState<AudioInputDeviceInfo[]>([]);
  const [microphoneDeviceId, setMicrophoneDeviceId] = useState("");
  const [isStartingRecording, setIsStartingRecording] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let disposed = false;

    const refreshDevices = async () => {
      try {
        const [devices, config] = await Promise.all([
          invoke<AudioInputDeviceInfo[]>("list_audio_input_devices"),
          invoke<StoredMicrophoneConfig>("load_config"),
        ]);
        if (disposed) return;
        setAudioInputDevices(devices);
        setMicrophoneDeviceId((current) => {
          const preferred = current || config.microphone_device_id || "";
          return resolveMicrophoneDeviceId(preferred, devices);
        });
      } catch (error) {
        console.error("Failed to refresh microphone selection:", error);
      }
    };

    void refreshDevices();
    const timer = window.setInterval(() => void refreshDevices(), 2500);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [enabled]);

  return {
    recordAudio,
    setRecordAudio,
    recordSystemAudio,
    setRecordSystemAudio,
    recordingFps,
    setRecordingFps,
    audioInputDevices,
    microphoneDeviceId,
    setMicrophoneDeviceId,
    isStartingRecording,
    setIsStartingRecording,
  };
}
