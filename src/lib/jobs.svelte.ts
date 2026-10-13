import { listen } from "@tauri-apps/api/event";
import type { JobInfo } from "./api";
import { log } from "./state.svelte";
import { t, te } from "./i18n.svelte";

export interface JobView extends JobInfo {
  rate: number;
}

export const jobs = $state({ list: [] as JobView[] });

const waiters = new Map<number, ((j: JobInfo) => void)[]>();
const finished = new Map<number, JobInfo>();
const samples = new Map<number, { bytes: number; t: number }>();

export const isFinished = (j: JobInfo) => j.state === "done" || j.state === "error" || j.state === "cancelled";

/** Job labels come from the backend as "<Verb>: <name>" or "<Verb>: <n> Elemente". */
export function jobLabel(label: string): string {
  return te(label).replace(/: (\d+) Elemente$/, (_, n) => ": " + t("{n} Elemente", { n }));
}

function handle(j: JobInfo) {
  const i = jobs.list.findIndex((x) => x.id === j.id);
  if (i >= 0 && isFinished(jobs.list[i])) return;

  const now = performance.now();
  const prev = samples.get(j.id);
  const prevRate = i >= 0 ? jobs.list[i].rate : 0;
  let rate = prevRate;
  if (prev && now - prev.t >= 200) {
    rate = prevRate * 0.5 + (((j.bytes - prev.bytes) / (now - prev.t)) * 1000) * 0.5;
    samples.set(j.id, { bytes: j.bytes, t: now });
  } else if (!prev) samples.set(j.id, { bytes: j.bytes, t: now });

  const view = { ...j, rate };
  if (i >= 0) jobs.list[i] = view;
  else jobs.list.push(view);

  if (!isFinished(j)) return;
  samples.delete(j.id);
  const label = jobLabel(j.label);
  for (const e of j.errors.slice(0, 50)) log(`${label}: ${te(e)}`, "error");
  if (j.errors.length > 50) log(`${label}: ${t("{n} weitere Fehler", { n: j.errors.length - 50 })}`, "error");
  if (j.state === "cancelled") log(`${label}: ${t("abgebrochen")} (${j.done}/${j.total})`);
  else if (j.state === "done") log(`${label}: ${t("{n} Datei(en) fertig", { n: j.done })}`, "ok");
  else log(`${label}: ${t("{done} fertig, {failed} fehlgeschlagen", { done: j.done, failed: j.failed })}`, "error");

  finished.set(j.id, j);
  for (const w of waiters.get(j.id) ?? []) w(j);
  waiters.delete(j.id);
  if (j.state !== "error") setTimeout(() => dismissJob(j.id), 4000);
}

export function dismissJob(id: number) {
  jobs.list = jobs.list.filter((x) => x.id !== id);
}

export function initJobs() {
  return listen<JobInfo>("job", ({ payload }) => handle(payload));
}

/** Resolves once the backend reports the job as finished. */
export function waitJob(id: number): Promise<JobInfo> {
  const done = finished.get(id);
  if (done) return Promise.resolve(done);
  return new Promise((resolve) => waiters.set(id, [...(waiters.get(id) ?? []), resolve]));
}
