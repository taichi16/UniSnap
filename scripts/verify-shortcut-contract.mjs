import { readFile } from "node:fs/promises";

const backend = await readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
const frontend = await readFile(new URL("../src/components/MainWindow.tsx", import.meta.url), "utf8");

const requirements = [
  [
    backend.includes("ShortcutRegistrationState"),
    "後端必須序列化快捷鍵重新註冊，避免連續變更互相交錯。",
  ],
  [
    !backend.includes("unregister_all()"),
    "不可在變更使用者快捷鍵時清除 Escape 等固定快捷鍵。",
  ],
  [
    backend.includes("restore_errors"),
    "新快捷鍵註冊失敗時必須恢復原快捷鍵。",
  ],
  [
    frontend.includes("shortcutSaveInFlight"),
    "前端必須阻止快捷鍵儲存要求重疊執行。",
  ],
  [
    frontend.includes("if (e.repeat) return"),
    "快捷鍵欄位必須忽略按鍵長按產生的重複事件。",
  ],
  [
    backend.includes("CloseToTrayState") && frontend.includes('invoke("quit_application")'),
    "程式必須提供不受常駐系統匣設定影響的明確退出路徑。",
  ],
];

const failures = requirements.filter(([passed]) => !passed).map(([, message]) => message);
if (failures.length > 0) {
  console.error(failures.map((message) => `- ${message}`).join("\n"));
  process.exit(1);
}

console.log("Shortcut safety contract verified.");
