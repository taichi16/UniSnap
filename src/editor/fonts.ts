export interface FontOption {
  label: string;
  value: string;
  source: "built-in" | "system";
}

export const DEFAULT_TEXT_FONT = "Inter, sans-serif";
export const WINDOWS_FONT_FALLBACK = '"Microsoft JhengHei UI", "Microsoft JhengHei", sans-serif';

export const BUILT_IN_FONT_OPTIONS: FontOption[] = [
  { label: "預設 (Inter)", value: DEFAULT_TEXT_FONT, source: "built-in" },
  { label: "Arial", value: "Arial, sans-serif", source: "built-in" },
  { label: "Georgia", value: "Georgia, serif", source: "built-in" },
  { label: "Courier New", value: "Courier New, monospace", source: "built-in" },
  {
    label: "微軟正黑體",
    value: WINDOWS_FONT_FALLBACK,
    source: "built-in",
  },
];

export function cssFontFamily(fontName: string): string {
  const escaped = fontName.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `"${escaped}", ${WINDOWS_FONT_FALLBACK}`;
}

export function mergeSystemFontOptions(fontNames: string[]): FontOption[] {
  const existingLabels = new Set(BUILT_IN_FONT_OPTIONS.map((option) => option.label.toLocaleLowerCase()));
  const systemOptions = fontNames
    .map((name) => name.trim())
    .filter((name) => name && !existingLabels.has(name.toLocaleLowerCase()))
    .map((name) => ({ label: name, value: cssFontFamily(name), source: "system" as const }));
  return [...BUILT_IN_FONT_OPTIONS, ...systemOptions];
}
