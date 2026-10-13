import { api } from "./api";

/** Error text the backend returns while the master password is locked. */
export const LOCKED = "Master-Passwort erforderlich";

export type MasterMode = "setup" | "unlock";

export const master = $state({
  configured: true,
  unlocked: false,
  email: "",
  dialog: null as MasterMode | null,
});

let waiting: ((ok: boolean) => void)[] = [];

export async function refreshMaster() {
  try {
    Object.assign(master, await api.masterStatus());
  } catch {
    /* keep the last known state */
  }
  if (!master.configured) master.dialog = "setup";
}

/** Asks for the master password if needed; resolves false when the user cancels. */
export async function ensureUnlocked(): Promise<boolean> {
  await refreshMaster();
  if (master.unlocked) return true;
  master.dialog = master.configured ? "unlock" : "setup";
  return new Promise((resolve) => waiting.push(resolve));
}

/** Called by the dialog when it closes. */
export function finishMaster(ok: boolean) {
  master.dialog = null;
  if (ok) master.unlocked = true;
  const w = waiting;
  waiting = [];
  w.forEach((f) => f(ok));
  refreshMaster();
}

/** Runs `fn`; if the backend reports the master password as locked, asks for it and retries once. */
export async function withUnlock<T>(fn: () => Promise<T>): Promise<T> {
  try {
    return await fn();
  } catch (e) {
    if (String(e) !== LOCKED) throw e;
    master.unlocked = false;
    if (!(await ensureUnlocked())) throw e;
    return fn();
  }
}

export async function lockMaster() {
  await api.masterLock();
  master.unlocked = false;
}
