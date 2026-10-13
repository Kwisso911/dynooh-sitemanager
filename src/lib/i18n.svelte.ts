import { invoke } from "@tauri-apps/api/core";
import { EN } from "./lang/en";
import { prefs } from "./prefs.svelte";

/** German source text → translation. Missing entries fall back to the German text. */
export interface LangPack {
  code: string;
  name: string;
  strings: Record<string, string>;
  builtin?: boolean;
}

const BUILTIN: LangPack[] = [
  { code: "de", name: "Deutsch", strings: {}, builtin: true },
  { code: "en", name: "English", strings: EN, builtin: true },
];

export const i18n = $state({ packs: [...BUILTIN] as LangPack[] });

export async function loadLangPacks() {
  patternCache.clear();
  try {
    const extra = await invoke<LangPack[]>("lang_list");
    const byCode = new Map(BUILTIN.map((p) => [p.code, p]));
    for (const p of extra) {
      const base = byCode.get(p.code);
      byCode.set(p.code, { ...p, strings: { ...(base?.strings ?? {}), ...p.strings }, name: p.name || base?.name || p.code });
    }
    i18n.packs = [...byCode.values()];
  } catch {
    i18n.packs = [...BUILTIN];
  }
}

function lookup(s: string): string | undefined {
  return i18n.packs.find((p) => p.code === prefs.lang)?.strings[s] || undefined;
}

/** Translates a UI text; `{name}` placeholders are filled from `params`. */
export function t(s: string, params?: Record<string, string | number>): string {
  const out = lookup(s) ?? s;
  return params ? out.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m)) : out;
}

const patternCache = new Map<string, [RegExp, string[], string][]>();

/** Entries with `{name}` placeholders, compiled so backend messages with values filled in can be matched. */
function patterns(): [RegExp, string[], string][] {
  const pack = i18n.packs.find((p) => p.code === prefs.lang);
  if (!pack) return [];
  let list = patternCache.get(pack.code);
  if (!list || list.length === 0) {
    list = Object.entries(pack.strings)
      .filter(([k, v]) => v && k.includes("{"))
      .map(([k, v]) => {
        const names: string[] = [];
        const src = k.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/\\\{(\w+)\\\}/g, (_, n) => (names.push(n), "(.+?)"));
        return [new RegExp(`^${src}$`, "s"), names, v] as [RegExp, string[], string];
      });
    patternCache.set(pack.code, list);
  }
  return list;
}

function matchPattern(s: string): string | undefined {
  for (const [re, names, out] of patterns()) {
    const m = re.exec(s);
    if (m) return out.replace(/\{(\w+)\}/g, (all, k) => (names.includes(k) ? te(m[names.indexOf(k) + 1]) : all));
  }
  return undefined;
}

/** Translates a message from the backend: whole text, a placeholder pattern, or the part before the first ": ". */
export function te(e: unknown): string {
  const s = String(e);
  const whole = lookup(s) ?? matchPattern(s);
  if (whole) return whole;
  const i = s.indexOf(": ");
  if (i > 0) {
    const head = lookup(s.slice(0, i));
    if (head) return head + ": " + te(s.slice(i + 2));
  }
  return s;
}

export const locale = () => (prefs.lang === "de" ? "de-CH" : prefs.lang);
