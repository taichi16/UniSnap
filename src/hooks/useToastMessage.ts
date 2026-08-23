import { useRef, useState } from "react";

export function useToastMessage(durationMs = 2000) {
  const [message, setMessage] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);

  const showToast = (nextMessage: string) => {
    setMessage(nextMessage);
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      setMessage(null);
      timerRef.current = null;
    }, durationMs);
  };

  return { toastMessage: message, showToast };
}
