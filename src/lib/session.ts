import { loadLayout, saveLayout } from "./drag";
import type { Tab } from "./state.svelte";

export interface PaneSession {
  local: string[];
  remote: string[];
  selLocal: { rel: string; isDir: boolean }[];
  selRemote: { rel: string; isDir: boolean }[];
}

interface Session {
  currentId: string;
  tab: Tab;
  linked: boolean;
  trees: Record<string, PaneSession>;
  files: { siteId: string; rel: string }[];
  active: string;
  views: Record<string, unknown>;
  /** Scroll position per `siteId:side`. */
  scroll: Record<string, number>;
}

/** Last UI state, restored on startup. Plain object: mutate, then call saveSession(). */
export const session: Session = {
  currentId: "",
  tab: "files",
  linked: true,
  trees: {},
  files: [],
  active: "",
  views: {},
  scroll: {},
  ...loadLayout<Partial<Session>>("session", {}),
};

let timer: ReturnType<typeof setTimeout> | undefined;

export function saveSession() {
  clearTimeout(timer);
  timer = setTimeout(() => saveLayout("session", session), 300);
}

export function flushSession() {
  clearTimeout(timer);
  saveLayout("session", session);
}
