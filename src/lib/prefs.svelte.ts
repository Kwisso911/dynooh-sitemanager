import { loadLayout, saveLayout } from "./drag";

export type ThemeName = "dark" | "light" | "custom";
export type Position = "left" | "right";

/** Editable colours; the rest of the palette is derived from these. */
export const COLOR_KEYS = ["bg", "bg2", "bg3", "line", "text", "muted", "accent", "ok", "warn", "err"] as const;
export type ColorKey = (typeof COLOR_KEYS)[number];
export type Palette = Record<ColorKey, string>;

export const THEMES: Record<"dark" | "light", Palette> = {
  dark: {
    bg: "#1e1f22",
    bg2: "#26282c",
    bg3: "#2f3136",
    line: "#3a3d43",
    text: "#d8dadf",
    muted: "#8a8f98",
    accent: "#3d8bfd",
    ok: "#4caf7a",
    warn: "#e0a43a",
    err: "#e5534b",
  },
  light: {
    bg: "#ffffff",
    bg2: "#f3f4f6",
    bg3: "#e7e9ed",
    line: "#d4d7dd",
    text: "#1f2328",
    muted: "#6b7280",
    accent: "#2563eb",
    ok: "#1a7f37",
    warn: "#b45309",
    err: "#cf222e",
  },
};

export interface Prefs {
  lang: string;
  /** Set once the user picks a language; until then English is used. */
  langChosen?: boolean;
  theme: ThemeName;
  custom: Palette;
  sidebar: Position;
  serverSide: Position;
  updateCheck: boolean;
}

const defaultLang = "en";

const stored = loadLayout<Partial<Prefs>>("prefs", {});

export const prefs = $state<Prefs>({
  theme: "dark",
  sidebar: "right",
  serverSide: "left",
  updateCheck: true,
  ...stored,
  lang: stored.langChosen && stored.lang ? stored.lang : defaultLang,
  custom: { ...THEMES.dark, ...(stored.custom ?? {}) },
});

export function savePrefs() {
  saveLayout("prefs", $state.snapshot(prefs));
  applyTheme();
}

export function palette(): Palette {
  return prefs.theme === "custom" ? prefs.custom : THEMES[prefs.theme];
}

function luminance(hex: string): number {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return 0;
  const n = parseInt(m[1], 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => c / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function isDark(): boolean {
  return luminance(palette().bg) < 0.5;
}

export function applyTheme() {
  if (typeof document === "undefined") return;
  const p = palette();
  const dark = isDark();
  const root = document.documentElement.style;
  for (const k of COLOR_KEYS) root.setProperty(`--${k}`, p[k]);
  root.setProperty("--sel", `color-mix(in srgb, ${p.accent} ${dark ? 35 : 22}%, ${p.bg})`);
  root.setProperty("--sel-focus", `color-mix(in srgb, ${p.accent} ${dark ? 55 : 35}%, ${p.bg})`);
  root.setProperty("--hover", `color-mix(in srgb, ${p.text} 6%, ${p.bg})`);
  root.setProperty("--danger-line", `color-mix(in srgb, ${p.err} 45%, ${p.bg})`);
  root.setProperty("--danger-text", dark ? `color-mix(in srgb, ${p.err} 60%, #ffffff)` : p.err);
  root.setProperty("color-scheme", dark ? "dark" : "light");
}
