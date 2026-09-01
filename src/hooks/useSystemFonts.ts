import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BUILT_IN_FONT_OPTIONS, mergeSystemFontOptions, type FontOption } from "../editor/fonts";

interface SystemFontsState {
  options: FontOption[];
  loading: boolean;
  error: string | null;
}

export function useSystemFonts(enabled: boolean): SystemFontsState {
  const attempted = useRef(false);
  const mounted = useRef(true);
  const [state, setState] = useState<SystemFontsState>({
    options: BUILT_IN_FONT_OPTIONS,
    loading: false,
    error: null,
  });

  useEffect(() => {
    // React Strict Mode intentionally runs an effect setup/cleanup cycle twice
    // in development. Restore the flag on every setup so a completed native
    // font request is not discarded after the simulated cleanup.
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    if (!enabled || attempted.current) return;

    attempted.current = true;
    setState((current) => ({ ...current, loading: true, error: null }));
    void invoke<string[]>("list_system_fonts")
      .then((fontNames) => {
        if (mounted.current) {
          setState({ options: mergeSystemFontOptions(fontNames), loading: false, error: null });
        }
      })
      .catch((error) => {
        if (mounted.current) {
          setState({
            options: BUILT_IN_FONT_OPTIONS,
            loading: false,
            error: `無法讀取系統字型：${String(error)}`,
          });
        }
      });
  }, [enabled]);

  return state;
}
