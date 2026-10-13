<script lang="ts">
  import { untrack } from "svelte";
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import { api, type Site } from "$lib/api";
  import { log, ui } from "$lib/state.svelte";
  import { t, te } from "$lib/i18n.svelte";

  let { site }: { site: Site } = $props();

  let draft = $state<Site>(untrack(() => structuredClone($state.snapshot(site))));
  let password = $state("");
  let excludeText = $state("");
  let busy = $state(false);
  let testResult = $state("");
  const groups = $derived([...new Set(ui.sites.map((s) => s.group).filter(Boolean))].sort((a, b) => a.localeCompare(b, "de")));

  $effect(() => {
    void site.id;
    untrack(() => {
      draft = structuredClone($state.snapshot(site));
      if (!draft.connections) draft.connections = 4;
      excludeText = site.exclude.join(", ");
      password = "";
      testResult = "";
    });
  });

  function collect(): Site {
    return {
      ...$state.snapshot(draft),
      port: Number(draft.port) || 21,
      connections: Math.min(10, Math.max(1, Math.round(Number(draft.connections) || 4))),
      group: (draft.group ?? "").trim(),
      exclude: excludeText.split(/[,\s]+/).map((s) => s.trim()).filter(Boolean),
    };
  }

  async function pickFolder() {
    const dir = await open({ directory: true, defaultPath: draft.localPath || undefined });
    if (typeof dir === "string") draft.localPath = dir;
  }

  async function save() {
    busy = true;
    try {
      const saved = await api.siteSave(collect(), password);
      ui.sites = await api.sitesList();
      password = "";
      log(t("Site gespeichert: {name}", { name: saved.name }), "ok");
    } catch (e) {
      log(`${t("Speichern")}: ${te(e)}`, "error");
    } finally {
      busy = false;
    }
  }

  async function test() {
    busy = true;
    testResult = t("Verbinde …");
    try {
      testResult = te(await api.siteTest(collect(), password));
    } catch (e) {
      testResult = `${t("Fehler")}: ${te(e)}`;
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!(await confirm(t("Site «{name}» aus dem SiteManager entfernen? Lokale Dateien bleiben erhalten.", { name: site.name }), { kind: "warning" }))) return;
    await api.siteDelete(site.id);
    ui.sites = await api.sitesList();
    if (!ui.sites.some((s) => s.id === ui.currentId)) ui.currentId = ui.sites[0]?.id ?? "";
    log(t("Site entfernt: {name}", { name: site.name }), "ok");
  }
</script>

<form class="site-form" onsubmit={(e) => { e.preventDefault(); save(); }}>
  <fieldset>
    <legend>{t("Site")}</legend>
    <div class="grid2">
      <label>{t("Name")} <input bind:value={draft.name} required /></label>
      <label>
        {t("Gruppe")}
        <input bind:value={draft.group} list="site-groups-{site.id}" placeholder={t("keine")} />
        <datalist id="site-groups-{site.id}">
          {#each groups as g}<option value={g}></option>{/each}
        </datalist>
      </label>
    </div>
    <label>
      {t("Lokaler Ordner")}
      <span class="row"><input bind:value={draft.localPath} required /><button type="button" onclick={pickFolder}>{t("Wählen …")}</button></span>
    </label>
    <label>{t("Web-URL")} <input bind:value={draft.webUrl} placeholder="https://…" /></label>
  </fieldset>

  <fieldset>
    <legend>{t("FTP-Server")}</legend>
    <div class="grid2">
      <label>{t("Host")} <input bind:value={draft.host} placeholder="ftp.example.ch" /></label>
      <label>{t("Port")} <input type="number" bind:value={draft.port} min="1" max="65535" /></label>
      <label>{t("Benutzer")} <input bind:value={draft.user} autocomplete="off" /></label>
      <label>
        {t("Passwort")}
        <input type="password" bind:value={password} autocomplete="new-password" placeholder={site.hasPassword ? t("gespeichert – leer lassen, um es zu behalten") : ""} />
      </label>
    </div>
    <label>{t("Remote-Ordner")} <input bind:value={draft.remotePath} placeholder="/httpdocs" /></label>
    <div class="grid2">
      <label class="check"><input type="checkbox" bind:checked={draft.passive} /> {t("Passiver Modus (PASV)")}</label>
      <label>
        {t("Gleichzeitige Verbindungen (1–10)")}
        <input type="number" bind:value={draft.connections} min="1" max="10" />
      </label>
    </div>
    <p class="muted">{t("Eine Verbindung bleibt für die Dateiliste reserviert; Transfers nutzen die übrigen.")}</p>
  </fieldset>

  <fieldset>
    <legend>{t("Optionen")}</legend>
    <label>
      {t("Ausschliessen (Namen oder Endungen, mit Komma getrennt)")}
      <input bind:value={excludeText} placeholder=".psd, .fla, cache" />
    </label>
    <p class="muted">{t("Immer ausgeschlossen:")} .git, node_modules, _notes, .DS_Store, Thumbs.db, desktop.ini</p>
    <label class="check"><input type="checkbox" bind:checked={draft.uploadOnSave} /> {t("Beim Speichern im Editor automatisch hochladen")}</label>
  </fieldset>

  <div class="row">
    <button class="primary" type="submit" disabled={busy}>{t("Speichern")}</button>
    <button type="button" onclick={test} disabled={busy}>{t("Verbindung testen")}</button>
    <div class="spacer"></div>
    <button type="button" class="danger" onclick={remove} disabled={busy}>{t("Site entfernen")}</button>
  </div>
  {#if testResult}<pre class="output">{testResult}</pre>{/if}
  {#if site.source}
    <p class="muted">
      {t("Quelle")}: {site.source === "dreamweaver" ? t("Dreamweaver-Import") : site.source === "folder" ? t("Ordner-Import") : site.source === "export" ? t("Import") : site.source}
    </p>
  {/if}
</form>
