<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import { currentSite, log, ui } from "$lib/state.svelte";
  import { t, te } from "$lib/i18n.svelte";
  import { ensureUnlocked } from "$lib/master.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let chosen = $state<Record<string, boolean>>(Object.fromEntries(ui.sites.map((s) => [s.id, true])));
  let password = $state("");
  let repeat = $state("");
  let busy = $state(false);

  const groups = $derived.by(() => {
    const names = [...new Set(ui.sites.map((s) => s.group))].sort((a, b) => (!a ? 1 : !b ? -1 : a.localeCompare(b, "de")));
    return names.map((name) => ({ name, sites: ui.sites.filter((s) => s.group === name) }));
  });
  const ids = $derived(ui.sites.filter((s) => chosen[s.id]).map((s) => s.id));
  const error = $derived(
    password.length > 0 && password.length < 8
      ? t("Mindestens 8 Zeichen")
      : repeat && repeat !== password
        ? t("Die Passwörter stimmen nicht überein")
        : "",
  );
  const ready = $derived(ids.length > 0 && password.length >= 8 && password === repeat && !busy);

  function setAll(on: boolean, list = ui.sites) {
    for (const s of list) chosen[s.id] = on;
  }

  async function run() {
    if (!ready || !(await ensureUnlocked())) return;
    const single = ids.length === 1 ? ui.sites.find((s) => s.id === ids[0]) : currentSite();
    const stamp = new Date().toISOString().slice(0, 10);
    const base = ids.length === 1 && single ? single.name.replace(/[\\/:*?"<>|]+/g, "_") : "sites";
    const path = await save({
      title: t("Export speichern"),
      defaultPath: `${base}-${stamp}.vfsites`,
      filters: [{ name: "SiteManager", extensions: ["vfsites"] }],
    });
    if (!path) return;
    busy = true;
    try {
      const n = await api.exportSites(ids, path, password);
      log(t("{n} Site(s) exportiert: {path}", { n, path }), "ok");
      onclose();
    } catch (e) {
      log(`${t("Export")}: ${te(e)}`, "error");
    } finally {
      busy = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape" && !busy) onclose();
  }
</script>

<svelte:window onkeydown={key} />

<div class="modal-backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !busy && onclose()}>
  <form class="modal export-modal" onsubmit={(e) => (e.preventDefault(), run())}>
    <div class="modal-head">
      <strong>{t("Sites exportieren")}</strong>
      <div class="spacer"></div>
      <button type="button" class="icon" title={t("Schliessen")} onclick={onclose} disabled={busy}>×</button>
    </div>
    <p class="muted">
      {t("Die Exportdatei enthält die gewählten Sites mit FTP-Zugang und allen Logins inklusive Passwörtern. Sie wird mit dem Export-Passwort verschlüsselt (AES-256-GCM, Argon2id).")}
    </p>
    <div class="row">
      <strong>{t("{n} von {total} gewählt", { n: ids.length, total: ui.sites.length })}</strong>
      <div class="spacer"></div>
      <button type="button" onclick={() => setAll(true)}>{t("Alle")}</button>
      <button type="button" onclick={() => setAll(false)}>{t("Keine")}</button>
    </div>
    <div class="export-list">
      {#each groups as g (g.name)}
        {@const all = g.sites.every((s) => chosen[s.id])}
        <label class="check export-group">
          <input type="checkbox" checked={all} onchange={() => setAll(!all, g.sites)} />
          {g.name || t("Ohne Gruppe")} <span class="muted">{g.sites.length}</span>
        </label>
        {#each g.sites as s (s.id)}
          <label class="check export-site">
            <input type="checkbox" bind:checked={chosen[s.id]} />
            {s.name} <span class="muted">{s.host}</span>
          </label>
        {/each}
      {/each}
    </div>
    <div class="grid2">
      <label>{t("Export-Passwort")} <input type="password" autocomplete="new-password" bind:value={password} /></label>
      <label>{t("Passwort wiederholen")} <input type="password" autocomplete="new-password" bind:value={repeat} /></label>
    </div>
    {#if error}<p class="error-text">{error}</p>{/if}
    <div class="row">
      <span class="muted">{t("Ohne dieses Passwort lässt sich der Export nicht mehr öffnen.")}</span>
      <div class="spacer"></div>
      <button type="button" onclick={onclose} disabled={busy}>{t("Abbrechen")}</button>
      <button class="primary" type="submit" disabled={!ready}>{busy ? t("Verschlüssle …") : t("Exportieren …")}</button>
    </div>
  </form>
</div>
