import { readdir, readFile } from "node:fs/promises";
import process from "node:process";

const libSource = await readFile("src-tauri/src/lib.rs", "utf8");
const permissionSource = await readFile(
  "src-tauri/permissions/app-permissions.toml",
  "utf8",
);

const handlerBlock = libSource.match(
  /generate_handler!\[([\s\S]*?)\]\)/,
)?.[1];
if (!handlerBlock) {
  throw new Error("找不到 generate_handler! command 清單");
}

const registered = new Set();
for (const rawLine of handlerBlock.split("\n")) {
  const token = rawLine.replace(/\/\/.*$/, "").replace(/[,\s]/g, "");
  if (!token) continue;
  registered.add(token.includes("::") ? token.split("::").at(-1) : token);
}

const allowlistBlock = permissionSource.match(
  /commands\.allow\s*=\s*\[([\s\S]*?)\]/,
)?.[1];
if (!allowlistBlock) {
  throw new Error("找不到 commands.allow allowlist");
}

const allowed = new Set(
  [...allowlistBlock.matchAll(/^\s*"([^"]+)"\s*,?\s*$/gm)].map(
    ([, command]) => command,
  ),
);

async function sourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) files.push(...(await sourceFiles(path)));
    else if (/\.(ts|tsx)$/.test(entry.name)) files.push(path);
  }
  return files;
}

const frontend = new Set();
for (const path of await sourceFiles("src")) {
  const source = await readFile(path, "utf8");
  for (const [, command] of source.matchAll(/invoke\(\s*["'`]([^"'`]+)["'`]/g)) {
    frontend.add(command);
  }
}

const missing = [...registered].filter((command) => !allowed.has(command));
const stale = [...allowed].filter((command) => !registered.has(command));
const unregisteredFrontend = [...frontend].filter((command) => !registered.has(command));

if (missing.length || stale.length || unregisteredFrontend.length) {
  if (missing.length) console.error(`allowlist 缺少：${missing.join(", ")}`);
  if (stale.length) console.error(`allowlist 多餘：${stale.join(", ")}`);
  if (unregisteredFrontend.length) {
    console.error(`前端呼叫未註冊：${unregisteredFrontend.join(", ")}`);
  }
  process.exit(1);
}

console.log(
  `command contract OK: ${registered.size} registered, ${frontend.size} literal frontend invokes`,
);
