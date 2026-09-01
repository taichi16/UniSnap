import { readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const assertions = [
  ["src-tauri/src/system_fonts.rs", "DWriteCreateFactory", "Windows DirectWrite 字型列舉"],
  ["src-tauri/src/lib.rs", "system_fonts::list_system_fonts", "Tauri 字型命令註冊"],
  ["src-tauri/permissions/app-permissions.toml", '"list_system_fonts"', "字型命令權限"],
  ["src/hooks/useSystemFonts.ts", 'invoke<string[]>("list_system_fonts")', "前端字型載入"],
  ["src/hooks/useSystemFonts.ts", "mounted.current = true", "Strict Mode 載入狀態復原"],
  ["src/components/FontPicker.tsx", 'role="listbox"', "可搜尋完整字型清單"],
  ["src/components/CaptureWindow.tsx", "useSystemFonts", "編輯器字型整合"],
];

const missing = assertions.filter(([path, needle]) => !read(path).includes(needle));
if (missing.length > 0) {
  for (const [path, , label] of missing) console.error(`missing ${label}: ${path}`);
  process.exit(1);
}

console.log(`font contract OK: ${assertions.length} checks`);
