<script lang="ts">
  import { untrack } from "svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, type Login, type Site } from "$lib/api";
  import { log, ui } from "$lib/state.svelte";
  import { t, te } from "$lib/i18n.svelte";
  import { LOCKED, master, withUnlock } from "$lib/master.svelte";

  let { site }: { site: Site } = $props();

  let draft = $state<Login | null>(null);
  let password = $state("");
  let revealed = $state<Record<string, string>>({});
  let busy = $state(false);

  const PRESETS = ["Contao Backend", "WordPress Admin", "Plesk", "cPanel", "phpMyAdmin / Datenbank", "Hosting-Kundencenter", "E-Mail", "Google Analytics / Search Console", "Domain-Registrar"];

  $effect(() => {
    void site.id;
    untrack(() => {
      draft = null;
      password = "";
      revealed = {};
    });
  });

  function replaceSite(updated: Site) {
    const i = ui.sites.findIndex((s) => s.id === updated.id);
    if (i >= 0) ui.sites[i] = updated;
  }

  function newLogin() {
    draft = { id: "", label: "", url: site.webUrl, user: "", note: "", hasPassword: false };
    password = "";
  }

  function edit(l: Login) {
    draft = { ...l };
    password = "";
  }

  async function save() {
    if (!draft) return;
    busy = true;
    try {
      replaceSite(await api.loginSave(site.id, $state.snapshot(draft), password));
      log(t("Login gespeichert: {name}", { name: draft.label }), "ok");
      draft = null;
      password = "";
    } catch (e) {
      log(`${t("Login speichern")}: ${te(e)}`, "error");
    } finally {
      busy = false;
    }
  }

  async function remove(l: Login) {
    if (!(await confirm(t("Login «{name}» löschen?", { name: l.label }), { kind: "warning" }))) return;
    try {
      replaceSite(await api.loginDelete(site.id, l.id));
      if (draft?.id === l.id) draft = null;
    } catch (e) {
      log(`${t("Login löschen")}: ${te(e)}`, "error");
    }
  }

  async function copy(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      log(t("{what} kopiert", { what }), "ok");
    } catch (e) {
      log(`${t("Kopieren")}: ${te(e)}`, "error");
    }
  }

  const fetchPassword = (id: string, ftp: boolean) =>
    withUnlock(() => (ftp ? api.ftpPassword(site.id) : api.loginPassword(site.id, id)));

  async function copyPassword(id: string, ftp = false) {
    try {
      await copy(await fetchPassword(id, ftp), t("Passwort"));
    } catch (e) {
      if (String(e) !== LOCKED) log(te(e), "error");
    }
  }

  async function toggleReveal(id: string, ftp = false) {
    if (revealed[id] !== undefined) {
      delete revealed[id];
      return;
    }
    try {
      revealed[id] = await fetchPassword(id, ftp);
    } catch (e) {
      if (String(e) !== LOCKED) log(te(e), "error");
    }
  }

  $effect(() => {
    if (!master.unlocked) untrack(() => (revealed = {}));
  });

  function open(url: string) {
    if (!url) return;
    openUrl(/^https?:\/\//i.test(url) ? url : `https://${url}`);
  }
</script>

<div class="logins-view">
  <div class="toolbar">
    <button class="primary" onclick={newLogin}>{t("+ Neues Login")}</button>
    <span class="muted">{t("Zugänge für CMS-Backends, Hosting, Datenbanken usw. Passwörter liegen in der Windows-Anmeldeinformationsverwaltung.")}</span>
  </div>

  <div class="logins-body">
    <table class="files logins">
      <thead>
        <tr><th>{t("Bezeichnung")}</th><th>URL</th><th>{t("Benutzer")}</th><th>{t("Passwort")}</th><th>{t("Notiz")}</th><th></th></tr>
      </thead>
      <tbody>
        <tr class="ftp-row">
          <td>FTP</td>
          <td>{site.host}{site.port && site.port !== 21 ? `:${site.port}` : ""}</td>
          <td>
            {site.user}
            {#if site.user}<button class="mini" onclick={() => copy(site.user, t("Benutzer"))}>{t("kopieren")}</button>{/if}
          </td>
          <td>
            {#if site.hasPassword}
              <span class="pw">{revealed["__ftp"] ?? "••••••••"}</span>
              <button class="mini" onclick={() => toggleReveal("__ftp", true)}>{revealed["__ftp"] !== undefined ? t("verbergen") : t("zeigen")}</button>
              <button class="mini" onclick={() => copyPassword("", true)}>{t("kopieren")}</button>
            {:else}<span class="muted">–</span>{/if}
          </td>
          <td class="muted">{t("bearbeiten unter «Einstellungen»")}</td>
          <td></td>
        </tr>
        {#each site.logins as l (l.id)}
          <tr ondblclick={() => edit(l)}>
            <td>{l.label}</td>
            <td>{#if l.url}<button class="link" onclick={() => open(l.url)}>{l.url}</button>{/if}</td>
            <td>
              {l.user}
              {#if l.user}<button class="mini" onclick={() => copy(l.user, t("Benutzer"))}>{t("kopieren")}</button>{/if}
            </td>
            <td>
              {#if l.hasPassword}
                <span class="pw">{revealed[l.id] ?? "••••••••"}</span>
                <button class="mini" onclick={() => toggleReveal(l.id)}>{revealed[l.id] !== undefined ? t("verbergen") : t("zeigen")}</button>
                <button class="mini" onclick={() => copyPassword(l.id)}>{t("kopieren")}</button>
              {:else}<span class="muted">–</span>{/if}
            </td>
            <td class="note" title={l.note}>{l.note}</td>
            <td class="actions">
              <button class="mini" onclick={() => edit(l)}>{t("bearbeiten")}</button>
              <button class="mini danger" onclick={() => remove(l)}>{t("löschen")}</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>

    {#if draft}
      <form class="site-form login-form" onsubmit={(e) => { e.preventDefault(); save(); }}>
        <fieldset>
          <legend>{draft.id ? t("Login bearbeiten") : t("Neues Login")}</legend>
          <div class="grid2">
            <label>{t("Bezeichnung")} <input bind:value={draft.label} list="login-presets" required placeholder={t("z. B. Contao Backend")} /></label>
            <label>URL <input bind:value={draft.url} placeholder="https://example.ch/contao" /></label>
            <label>{t("Benutzer")} <input bind:value={draft.user} autocomplete="off" /></label>
            <label>
              {t("Passwort")}
              <input type="password" bind:value={password} autocomplete="new-password" placeholder={draft.hasPassword ? t("gespeichert – leer lassen, um es zu behalten") : ""} />
            </label>
          </div>
          <label>{t("Notiz")} <textarea bind:value={draft.note} rows="3" placeholder={t("z. B. 2FA über Handy von …, nur für Unterseite /intern")}></textarea></label>
          <datalist id="login-presets">
            {#each PRESETS as p}<option value={t(p)}></option>{/each}
          </datalist>
        </fieldset>
        <div class="row">
          <button class="primary" type="submit" disabled={busy}>{t("Speichern")}</button>
          <button type="button" onclick={() => (draft = null)}>{t("Abbrechen")}</button>
        </div>
      </form>
    {/if}
  </div>
</div>
