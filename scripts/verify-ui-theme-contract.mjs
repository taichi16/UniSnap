import { readFile } from "node:fs/promises";

const css = await readFile("src/App.css", "utf8");
const ocrComponent = await readFile("src/components/OcrResultModal.tsx", "utf8");
const captureComponent = await readFile("src/components/CaptureWindow.tsx", "utf8");
const contract = css.match(
  /THEME-INVARIANT-DARK-SURFACES-START([\s\S]*?)THEME-INVARIANT-DARK-SURFACES-END/,
)?.[1];

if (!contract) throw new Error("找不到固定深色介面的主題對比契約");

const requiredRules = [
  [/\.toolbar-floating \.toolbar-btn\s*\{[^}]*color:\s*#c4c9d4/i, "編輯工具列"],
  [/\.sub-toolbar\s*\{[^}]*color:\s*#f3f4f6/i, "工具選項"],
  [/\.ocr-result-modal[\s\S]*?\{[^}]*color:\s*#f3f4f6/i, "OCR／拼接視窗"],
  [/\.ocr-textarea\s*\{[^}]*color:\s*#f3f4f6/i, "OCR 文字區"],
  [/\.toast\s*\{[^}]*color:\s*#f3f4f6/i, "通知訊息"],
];

for (const [pattern, label] of requiredRules) {
  if (!pattern.test(contract)) throw new Error(`${label}缺少固定高對比前景色`);
}

if (!/<pre className="ocr-text-output"/.test(ocrComponent)
  || !/style\.setProperty\("-webkit-text-fill-color",\s*"#f8fafc",\s*"important"\)/.test(ocrComponent)
  || !/style\.setProperty\("opacity",\s*"1",\s*"important"\)/.test(ocrComponent)) {
  throw new Error("OCR 結果必須使用獨立文字輸出區，並在 DOM 層強制高對比文字樣式");
}

if (!/document\.documentElement\.dataset\.theme\s*=\s*"dark"/.test(captureComponent)
  || !/previousTheme/.test(captureComponent)) {
  throw new Error("開啟舊檔與長圖編輯器必須切換至深色編輯介面，並在離開時恢復原主題");
}

if (/color\s*:\s*var\(--text-(?:primary|secondary|muted)\)/i.test(contract)) {
  throw new Error("固定深色介面不得使用會隨主題切換的文字色彩變數");
}

console.log(`UI theme contract OK: ${requiredRules.length} dark surfaces, OCR and old-file editor route verified`);
