<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import { log } from "$lib/state.svelte";
  import { i18n, loadLangPacks, t, te } from "$lib/i18n.svelte";
  import { EN } from "$lib/lang/en";
  import { master, refreshMaster } from "$lib/master.svelte";
  import { COLOR_KEYS, THEMES, prefs, savePrefs, type ColorKey } from "$lib/prefs.svelte";

  let { onclose }: { onclose: () => void } = $props();

  const COLOR_LABEL: Record<ColorKey, string> = {
    bg: "Hintergrund und Eingabefelder",
    bg2: "Seitenleiste und Flächen",
    bg3: "Schaltflächen",
    line: "Linien und Rahmen",
    text: "Text",
    muted: "Nebentext",
    accent: "Akzent",
    ok: "Erfolg",
    warn: "Warnung",
    err: "Fehler",
  };

  $effect(() => {
    void JSON.stringify(prefs);
    savePrefs();
  });

  function copyFrom(name: "dark" | "light") {
    prefs.custom = { ...THEMES[name] };
  }

  async function installPack() {
    const path = await open({ title: t("Sprachpaket wählen"), filters: [{ name: "JSON", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      const name = await api.langInstall(path);
      await loadLangPacks();
      log(t("Sprachpaket installiert: {name}", { name }), "ok");
    } catch (e) {
      log(`${t("Sprachpaket")}: ${te(e)}`, "error");
    }
  }

  async function makeTemplate() {
    try {
      const path = await api.langTemplate(Object.keys(EN));
      await revealItemInDir(path);
      log(t("Vorlage erstellt: {path}", { path }), "ok");
    } catch (e) {
      log(`${t("Vorlage")}: ${te(e)}`, "error");
    }
  }

  let mCurrent = $state("");
  let mNew = $state("");
  let mRepeat = $state("");
  let mEmail = $state("");
  let mError = $state("");
  let mBusy = $state(false);

  async function changeMaster() {
    mError = "";
    if (mNew && mNew.length < 8) return void (mError = t("Mindestens 8 Zeichen"));
    if (mNew !== mRepeat) return void (mError = t("Die Passwörter stimmen nicht überein"));
    mBusy = true;
    try {
      await api.masterChange(mCurrent, mNew || null, mEmail.trim() || null);
      mCurrent = mNew = mRepeat = mEmail = "";
      await refreshMaster();
      log(t("Master-Passwort-Einstellungen gespeichert"), "ok");
    } catch (e) {
      mError = te(e);
    } finally {
      mBusy = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape" && !master.dialog) onclose();
  }
</script>

<svelte:window onkeydown={key} />

<div class="modal-backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal prefs-modal" role="dialog" aria-label={t("Programm-Einstellungen")}>
    <div class="modal-head">
      <strong>{t("Programm-Einstellungen")}</strong>
      <div class="spacer"></div>
      <button class="icon" title={t("Schliessen")} onclick={onclose}>×</button>
    </div>

    <fieldset>
      <legend>{t("Sprache")}</legend>
      <div class="row">
        <select bind:value={prefs.lang} onchange={() => (prefs.langChosen = true)} style="flex: 1">
          {#each i18n.packs as p (p.code)}
            <option value={p.code}>{p.name}{p.builtin ? "" : ` (${t("Sprachpaket")})`}</option>
          {/each}
        </select>
        <button onclick={installPack}>{t("Sprachpaket laden …")}</button>
      </div>
      <p class="muted">
        {t("Sprachpakete sind JSON-Dateien mit «code», «name» und «strings» (deutscher Text → Übersetzung).")}
        <button class="link" onclick={makeTemplate}>{t("Vorlage erstellen")}</button>
      </p>
    </fieldset>

    <fieldset>
      <legend>{t("Design")}</legend>
      <div class="row">
        <label class="check"><input type="radio" bind:group={prefs.theme} value="dark" /> {t("Dunkel")}</label>
        <label class="check"><input type="radio" bind:group={prefs.theme} value="light" /> {t("Hell")}</label>
        <label class="check"><input type="radio" bind:group={prefs.theme} value="custom" /> {t("Eigene Farben")}</label>
      </div>
      {#if prefs.theme === "custom"}
        <div class="color-grid">
          {#each COLOR_KEYS as k (k)}
            <label class="color-field">
              <input type="color" bind:value={prefs.custom[k]} />
              <span>{t(COLOR_LABEL[k])}</span>
            </label>
          {/each}
        </div>
        <div class="row">
          <span class="muted">{t("Ausgangslage übernehmen:")}</span>
          <button onclick={() => copyFrom("dark")}>{t("Dunkel")}</button>
          <button onclick={() => copyFrom("light")}>{t("Hell")}</button>
        </div>
      {/if}
    </fieldset>

    {#if master.configured}
      <fieldset>
        <legend>{t("Master-Passwort")}</legend>
        <p class="muted">{t("E-Mail für «Passwort vergessen»:")} {master.email}</p>
        <div class="grid2">
          <label>{t("Aktuelles Master-Passwort")} <input type="password" autocomplete="current-password" bind:value={mCurrent} /></label>
          <label>{t("Neue E-Mail-Adresse (optional)")} <input type="email" bind:value={mEmail} /></label>
          <label>{t("Neues Master-Passwort (optional)")} <input type="password" autocomplete="new-password" bind:value={mNew} /></label>
          <label>{t("Passwort wiederholen")} <input type="password" autocomplete="new-password" bind:value={mRepeat} /></label>
        </div>
        {#if mError}<p class="error-text">{mError}</p>{/if}
        <div class="row">
          <div class="spacer"></div>
          <button onclick={changeMaster} disabled={mBusy || !mCurrent || (!mNew && !mEmail.trim())}>{t("Ändern")}</button>
        </div>
      </fieldset>
    {/if}

    <fieldset>
      <legend>{t("Layout")}</legend>
      <div class="grid2">
        <label>
          {t("Site-Liste")}
          <select bind:value={prefs.sidebar}>
            <option value="left">{t("links")}</option>
            <option value="right">{t("rechts")}</option>
          </select>
        </label>
        <label>
          {t("Spalten in der Dateiansicht")}
          <select bind:value={prefs.panes}>
            <option value="both">{t("Server und lokal")}</option>
            <option value="remote">{t("Nur Server")}</option>
            <option value="local">{t("Nur lokal")}</option>
          </select>
        </label>
        <label>
          {t("Server-Spalte in der Dateiansicht")}
          <select bind:value={prefs.serverSide} disabled={prefs.panes !== "both"}>
            <option value="left">{t("links")}</option>
            <option value="right">{t("rechts")}</option>
          </select>
        </label>
      </div>
    </fieldset>

    <fieldset>
      <legend>{t("Updates")}</legend>
      <label class="check"><input type="checkbox" bind:checked={prefs.updateCheck} /> {t("Beim Start nach neuer Version suchen")}</label>
      <p class="muted">{t("Lädt dazu nur die Versionsnummer von apps.dynooh.com, es werden keine Daten gesendet.")}</p>
    </fieldset>
  </div>
</div>
