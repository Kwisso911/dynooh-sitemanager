import type { Site } from "./api";
import { locale } from "./i18n.svelte";

export type Tab = "files" | "editor" | "git" | "logins";

export interface LogLine {
  time: string;
  msg: string;
  level: "info" | "error" | "ok";
}

export interface OpenFile {
  key: string;
  siteId: string;
  rel: string;
  name: string;
  dirty: boolean;
}

interface PromptRequest {
  text: string;
  value: string;
  password?: boolean;
  resolve: (v: string | null) => void;
}

export const ui = $state({
  sites: [] as Site[],
  currentId: "",
  tab: "files" as Tab,
  log: [] as LogLine[],
  progress: null as null | { text: string; pct: number | null },
  prompt: null as PromptRequest | null,
});

export const editorState = $state({
  files: [] as OpenFile[],
  active: "",
});

export const settings = $state({
  open: [] as string[],
  active: "",
});

export function openSettings(siteId: string) {
  if (!settings.open.includes(siteId)) settings.open.push(siteId);
  settings.active = siteId;
}

export function closeSettings(siteId?: string) {
  if (siteId === undefined) {
    settings.open = [];
    settings.active = "";
    return;
  }
  const i = settings.open.indexOf(siteId);
  if (i < 0) return;
  settings.open.splice(i, 1);
  if (settings.active === siteId) settings.active = settings.open[Math.min(i, settings.open.length - 1)] ?? "";
}

export function currentSite(): Site | undefined {
  return ui.sites.find((s) => s.id === ui.currentId);
}

export function log(msg: string, level: LogLine["level"] = "info") {
  ui.log.unshift({ time: new Date().toLocaleTimeString(locale()), msg, level });
  if (ui.log.length > 500) ui.log.length = 500;
}

export function openFile(siteId: string, rel: string) {
  const key = `${siteId}::${rel}`;
  if (!editorState.files.some((f) => f.key === key)) {
    editorState.files.push({ key, siteId, rel, name: rel.split("/").pop() ?? rel, dirty: false });
  }
  editorState.active = key;
  ui.tab = "editor";
}

export function promptText(text: string, value = "", password = false): Promise<string | null> {
  return new Promise((resolve) => {
    ui.prompt = { text, value, password, resolve };
  });
}

const BINARY = /\.(png|jpe?g|gif|webp|avif|ico|bmp|tiff?|psd|ai|pdf|zip|rar|7z|gz|mp4|webm|mov|mp3|wav|ogg|woff2?|ttf|otf|eot|fla|swf|exe|dll)$/i;

export function isBinary(name: string): boolean {
  return BINARY.test(name);
}

export function fmtSize(n: number | null | undefined): string {
  if (n == null) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function fmtDate(secs: number | null | undefined): string {
  if (!secs) return "";
  return new Date(secs * 1000).toLocaleString(locale(), {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
