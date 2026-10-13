<script lang="ts">
  import { untrack } from "svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { api, type Side, type Site, type SyncItem, type SyncStatus } from "$lib/api";
  import { fmtDate, fmtSize, log } from "$lib/state.svelte";
  import { waitJob } from "$lib/jobs.svelte";
  import { t, te } from "$lib/i18n.svelte";

  type Action = "upload" | "download" | "delLocal" | "delRemote" | "skip";

  let { site, rel = "", onclose }: { site: Site; rel?: string; onclose?: () => void } = $props();

  let items = $state<(SyncItem & { action: Action })[]>([]);
  let busy = $state(false);
  let compared = $state(false);
  let showSame = $state(false);

  const LABEL: Record<SyncStatus, string> = {
    same: "Synchron",
    probablySame: "Gleiche Grösse (unbestätigt)",
    different: "Unterschiedlich (unbekannt)",
    upload: "Lokal geändert",
    download: "Server geändert",
    conflict: "Beide geändert",
    localOnly: "Nur lokal",
    remoteOnly: "Nur Server",
    localDeleted: "Lokal gelöscht",
    remoteDeleted: "Auf Server gelöscht",
  };

  const DEFAULT: Record<SyncStatus, Action> = {
    same: "skip",
    probablySame: "skip",
    different: "skip",
    upload: "upload",
    download: "download",
    conflict: "skip",
    localOnly: "upload",
    remoteOnly: "skip",
    localDeleted: "skip",
    remoteDeleted: "skip",
  };

  $effect(() => {
    void site.id;
    void rel;
    untrack(() => {
      items = [];
      compared = false;
      compare();
    });
  });

  async function compare() {
    busy = true;
    const id = site.id;
    const dir = rel;
    try {
      const res = await api.syncCompare(id, dir);
      if (site.id !== id || rel !== dir) return;
      items = res.map((i) => ({ ...i, action: DEFAULT[i.status] }));
      compared = true;
      log(t("Vergleich: {n} Dateien, {m} mit Aktion", { n: res.length, m: items.filter((i) => i.action !== "skip").length }), "ok");
    } catch (e) {
      log(`${t("Vergleich")}: ${te(e)}`, "error");
    } finally {
      busy = false;
    }
  }

  async function apply() {
    const uploads = items.filter((i) => i.action === "upload" && i.localSize != null).map((i) => i.rel);
    const downloads = items.filter((i) => i.action === "download" && i.remoteSize != null).map((i) => i.rel);
    const deletes = items
      .filter((i) => (i.action === "delLocal" && i.localSize != null) || (i.action === "delRemote" && i.remoteSize != null))
      .map((i) => {
        const side: Side = i.action === "delLocal" ? "local" : "remote";
        return { rel: i.rel, side, forget: side === "local" ? i.remoteSize == null : i.localSize == null };
      });
    if (uploads.length + downloads.length + deletes.length === 0) return;
    if (deletes.length) {
      const nl = deletes.filter((d) => d.side === "local").length;
      const nr = deletes.length - nl;
      const list = deletes.slice(0, 20).map((d) => `${d.side === "local" ? t("lokal") : t("Server")}: ${d.rel}`).join("\n");
      const more = deletes.length > 20 ? "\n" + t("… und {n} weitere", { n: deletes.length - 20 }) : "";
      const text = t("{local} Datei(en) lokal in den Papierkorb verschieben und {remote} Datei(en) auf dem Server endgültig löschen?", { local: nl, remote: nr });
      if (!(await confirm(`${text}\n\n${list}${more}`, { title: t("Synchronisieren"), kind: "warning" }))) return;
    }
    busy = true;
    try {
      const id = site.id;
      await waitJob(await api.syncApply(id, uploads, downloads, deletes));
      if (site.id === id) await compare();
      else busy = false;
    } catch (e) {
      log(`${t("Synchronisieren")}: ${te(e)}`, "error");
      busy = false;
    }
  }

  async function markSame() {
    const rels = items.filter((i) => i.status === "probablySame").map((i) => i.rel);
    if (rels.length === 0) return;
    busy = true;
    try {
      const n = await api.syncMarkSame(site.id, rels);
      log(t("{n} Datei(en) als synchron markiert", { n }), "ok");
      await compare();
    } catch (e) {
      log(`${t("Markieren")}: ${te(e)}`, "error");
      busy = false;
    }
  }

  function setAll(a: Action) {
    for (const i of visible) {
      if (a === "upload" && i.localSize == null) continue;
      if (a === "download" && i.remoteSize == null) continue;
      i.action = a;
    }
  }

  const prefix = $derived(rel ? rel + "/" : "");
  const shown = (r: string) => (prefix && r.startsWith(prefix) ? r.slice(prefix.length) : r);

  const visible = $derived(items.filter((i) => showSame || (i.status !== "same" && i.status !== "probablySame")));
  const counts = $derived({
    up: items.filter((i) => i.action === "upload").length,
    down: items.filter((i) => i.action === "download").length,
    del: items.filter((i) => i.action === "delLocal" || i.action === "delRemote").length,
    same: items.filter((i) => i.status === "probablySame").length,
  });
</script>

<div class="sync-view">
  <div class="toolbar">
    {#if onclose}
      <button onclick={onclose} title={t("Zurück zur Dateiansicht")}>← {t("Dateien")}</button>
      <span class="divider"></span>
    {/if}
    <span class="sync-scope" title={"/" + rel}>{t("Synchronisieren")}: <b>/{rel}</b></span>
    <span class="divider"></span>
    <button class="primary" onclick={compare} disabled={busy}>{compared ? t("Neu vergleichen") : t("Vergleichen")}</button>
    <button onclick={apply} disabled={busy || counts.up + counts.down + counts.del === 0}>
      {t("Ausführen")} ({counts.up} ↑ · {counts.down} ↓{counts.del ? ` · ${counts.del} ✕` : ""})
    </button>
    <span class="divider"></span>
    <button onclick={() => setAll("upload")} disabled={busy || !compared}>{t("Alle")} ↑</button>
    <button onclick={() => setAll("download")} disabled={busy || !compared}>{t("Alle")} ↓</button>
    <button onclick={() => setAll("skip")} disabled={busy || !compared}>{t("Keine")}</button>
    <span class="divider"></span>
    <button
      onclick={markSame}
      disabled={busy || counts.same === 0}
      title={t("Dateien mit gleicher Grösse lokal und auf dem Server als Ausgangslage übernehmen")}
    >
      {t("Gleich grosse als synchron markieren")} ({counts.same})
    </button>
    <label class="check"><input type="checkbox" bind:checked={showSame} /> {t("Synchrone anzeigen")}</label>
  </div>
  <div class="sync-body">
    {#if !compared}
      <div class="hint">
        {busy
          ? t("Lokale und Server-Dateien werden eingelesen …")
          : t("«Vergleichen» liest lokale und Server-Dateien rekursiv ein. Beim ersten Vergleich fehlt die Sync-Historie; Dateien mit gleicher Grösse können dann als synchron markiert werden.")}
      </div>
    {:else if visible.length === 0}
      <div class="hint">{t("Keine Unterschiede.")}</div>
    {:else}
      <table class="files">
        <thead>
          <tr>
            <th>{t("Datei")}</th><th>{t("Status")}</th><th>{t("Aktion")}</th>
            <th class="num">{t("Lokal")}</th><th>{t("Lokal geändert")}</th>
            <th class="num">{t("Server")}</th><th>{t("Server geändert")}</th>
          </tr>
        </thead>
        <tbody>
          {#each visible as i (i.rel)}
            <tr class="st-{i.status}">
              <td title={i.rel}>{shown(i.rel)}</td>
              <td>{t(LABEL[i.status])}</td>
              <td>
                <select bind:value={i.action} class:del-action={i.action === "delLocal" || i.action === "delRemote"}>
                  <option value="skip">–</option>
                  {#if i.localSize != null}<option value="upload">↑ {t("Hochladen")}</option>{/if}
                  {#if i.remoteSize != null}<option value="download">↓ {t("Herunterladen")}</option>{/if}
                  {#if i.localSize != null}<option value="delLocal">✕ {t("Lokal löschen")}</option>{/if}
                  {#if i.remoteSize != null}<option value="delRemote">✕ {t("Auf dem Server löschen")}</option>{/if}
                </select>
              </td>
              <td class="num">{fmtSize(i.localSize)}</td>
              <td>{fmtDate(i.localMtime)}</td>
              <td class="num">{fmtSize(i.remoteSize)}</td>
              <td>{fmtDate(i.remoteMtime)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</div>
