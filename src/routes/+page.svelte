<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, webUrlFor, type Progress, type Site, type UpdateInfo } from "$lib/api";
  import { currentSite, editorState, log, openSettings, promptText, ui, type Tab } from "$lib/state.svelte";
  import { initJobs } from "$lib/jobs.svelte";
  import { loadLangPacks, t, te } from "$lib/i18n.svelte";
  import { applyTheme, prefs } from "$lib/prefs.svelte";
  import FilesView from "$lib/components/FilesView.svelte";
  import EditorView from "$lib/components/EditorView.svelte";
  import GitView from "$lib/components/GitView.svelte";
  import LoginsView from "$lib/components/LoginsView.svelte";
  import SettingsCard from "$lib/components/SettingsCard.svelte";
  import TransferPanel from "$lib/components/TransferPanel.svelte";
  import PrefsDialog from "$lib/components/PrefsDialog.svelte";
  import ExportDialog from "$lib/components/ExportDialog.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import MasterDialog from "$lib/components/MasterDialog.svelte";
  import { ensureUnlocked, lockMaster, master, refreshMaster } from "$lib/master.svelte";
  import ContextMenu, { type MenuItem } from "$lib/components/ContextMenu.svelte";
  import { clamp, drag, loadLayout, saveLayout } from "$lib/drag";
  import { flushSession, saveSession, session } from "$lib/session";

  let filter = $state("");
  let sidebarWidth = $state(loadLayout("sidebar", 280));
  let showPrefs = $state(false);
  let showExport = $state(false);

  function resizeSidebar(e: PointerEvent) {
    const start = sidebarWidth;
    const dir = prefs.sidebar === "left" ? 1 : -1;
    drag(
      e,
      (dx) => (sidebarWidth = clamp(start + dir * dx, 180, 600)),
      () => saveLayout("sidebar", sidebarWidth),
    );
  }

  function selectSite(id: string) {
    ui.currentId = id;
    ui.tab = "files";
  }

  let siteMenu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function siteContext(e: MouseEvent, s: Site) {
    e.preventDefault();
    const items: MenuItem[] = [
      { label: t("Einstellungen"), action: () => openSettings(s.id) },
      { label: t("Gruppe …"), action: () => askGroup(s) },
      {
        label: t("Im Explorer öffnen"),
        action: () => api.openFolder(s.id).catch((err) => log(`Explorer: ${te(err)}`, "error")),
      },
    ];
    const url = webUrlFor(s);
    if (url) items.push({ label: t("Im Browser öffnen"), action: () => openUrl(url) });
    siteMenu = { x: e.clientX, y: e.clientY, items };
  }
  let showLog = $state(false);
  let promptInput = $state<HTMLInputElement | null>(null);

  const TABS: [Tab, string][] = [
    ["files", "Dateien"],
    ["editor", "Editor"],
    ["git", "Git"],
    ["logins", "Logins"],
  ];

  let restored = $state(false);
  let version = $state("");
  let update = $state<UpdateInfo | null>(null);

  function checkUpdate() {
    api
      .checkUpdate()
      .then((u) => {
        if (!u.newer) return;
        update = u;
        log(t("Neue Version {latest} verfügbar (installiert: {current})", { latest: u.latest, current: u.current }));
      })
      .catch(() => {});
  }

  onMount(() => {
    applyTheme();
    loadLangPacks();
    getVersion().then((v) => (version = v));
    if (prefs.updateCheck) checkUpdate();
    refreshMaster();
    const masterTimer = setInterval(refreshMaster, 30_000);
    const loaded = api.sitesList();
    loaded.finally(() => tick().then(() => api.appReady())).catch(() => {});
    loaded.then((s) => {
      ui.sites = s;
      const exists = (id: string) => s.some((x) => x.id === id);
      ui.currentId = exists(session.currentId) ? session.currentId : (s[0]?.id ?? "");
      editorState.files = session.files
        .filter((f) => exists(f.siteId))
        .map((f) => ({ key: `${f.siteId}::${f.rel}`, siteId: f.siteId, rel: f.rel, name: f.rel.split("/").pop() ?? f.rel, dirty: false }));
      editorState.active = editorState.files.some((f) => f.key === session.active) ? session.active : (editorState.files[0]?.key ?? "");
      const validTab = TABS.some(([id]) => id === session.tab);
      ui.tab = !validTab || (session.tab === "editor" && editorState.files.length === 0) ? "files" : session.tab;
      restored = true;
    });
    window.addEventListener("beforeunload", flushSession);
    const unJobs = initJobs();
    const un = listen<Progress>("progress", ({ payload: p }) => {
      if (p.error) log(`${p.rel}: ${te(p.error)}`, "error");
      if (p.kind === "done") ui.progress = null;
      else if (p.kind === "scan") ui.progress = { text: t("Lese Server: /{dir}", { dir: p.rel }), pct: null };
      else
        ui.progress = {
          text: `${p.index + 1}/${p.total}: ${p.rel}`,
          pct: p.total ? ((p.index + 1) / p.total) * 100 : null,
        };
    });
    return () => {
      clearInterval(masterTimer);
      un.then((f) => f());
      unJobs.then((f) => f());
    };
  });

  $effect(() => {
    document.documentElement.lang = prefs.lang;
  });

  $effect(() => {
    const snapshot = {
      currentId: ui.currentId,
      tab: ui.tab,
      files: editorState.files.map((f) => ({ siteId: f.siteId, rel: f.rel })),
      active: editorState.active,
    };
    if (!restored) return;
    Object.assign(session, snapshot);
    saveSession();
  });

  $effect(() => {
    if (ui.prompt && promptInput) {
      promptInput.focus();
      promptInput.select();
    }
  });

  function closePrompt(value: string | null) {
    const v = ui.prompt?.password ? value : value?.trim();
    ui.prompt?.resolve(v || null);
    ui.prompt = null;
  }

  async function newSite() {
    const site: Site = {
      id: "",
      name: t("Neue Site"),
      localPath: "",
      host: "",
      port: 21,
      user: "",
      remotePath: "",
      passive: true,
      exclude: [],
      webUrl: "",
      uploadOnSave: false,
      hasPassword: false,
      source: "",
      logins: [],
      connections: 4,
      group: currentSite()?.group ?? "",
    };
    const saved = await api.siteSave(site);
    ui.sites = await api.sitesList();
    ui.currentId = saved.id;
    openSettings(saved.id);
  }

  async function importDw() {
    try {
      const r = await api.importDreamweaver();
      ui.sites = await api.sitesList();
      log(t("Dreamweaver-Import: {added} neu, {updated} ergänzt, {skipped} unverändert", { ...r }), "ok");
    } catch (e) {
      log(`${t("Dreamweaver-Import")}: ${te(e)}`, "error");
    }
  }

  async function importFolders() {
    const root = await open({ directory: true, title: t("Ordner mit den Site-Ordnern wählen") });
    if (typeof root !== "string") return;
    try {
      const r = await api.importFolders(root);
      ui.sites = await api.sitesList();
      log(t("Ordner-Import: {added} neu, {skipped} bereits vorhanden", { ...r }), "ok");
    } catch (e) {
      log(`${t("Ordner-Import")}: ${te(e)}`, "error");
    }
  }

  async function importExport() {
    const path = await open({ title: t("Exportdatei wählen"), filters: [{ name: "SiteManager", extensions: ["vfsites", "json"] }] });
    if (typeof path !== "string") return;
    const password = await promptText(t("Passwort der Exportdatei"), "", true);
    if (!password) return;
    try {
      const r = await api.importSites(path, password);
      ui.sites = await api.sitesList();
      log(t("Import: {added} neu, {updated} aktualisiert", { ...r }), "ok");
    } catch (e) {
      log(`${t("Import")}: ${te(e)}`, "error");
    }
  }

  const site = $derived(currentSite());
  const siteUrl = $derived(site ? webUrlFor(site) : null);
  const filtered = $derived(
    ui.sites.filter((s) => {
      const f = filter.toLowerCase();
      return !f || s.name.toLowerCase().includes(f) || s.host.toLowerCase().includes(f);
    }),
  );
  const lastLog = $derived(ui.log[0]);

  const NEW_GROUP = "\u0000new";
  let emptyGroups = $state<string[]>(loadLayout("emptyGroups", []));
  const groupNames = $derived(
    [...new Set([...ui.sites.map((s) => s.group), ...emptyGroups].filter(Boolean))].sort((a, b) => a.localeCompare(b, "de")),
  );

  $effect(() => {
    const used = new Set(ui.sites.map((s) => s.group));
    if (emptyGroups.some((g) => used.has(g))) {
      emptyGroups = emptyGroups.filter((g) => !used.has(g));
      saveLayout("emptyGroups", emptyGroups);
    }
  });

  async function newGroup() {
    const name = await promptText(t("Name der neuen Gruppe"));
    if (!name || groupNames.includes(name)) return;
    emptyGroups = [...emptyGroups, name];
    saveLayout("emptyGroups", emptyGroups);
  }

  function removeEmptyGroup(g: string) {
    emptyGroups = emptyGroups.filter((x) => x !== g);
    saveLayout("emptyGroups", emptyGroups);
  }
  const grouped = $derived.by(() => {
    const out = groupNames.map((name) => ({ name, sites: filtered.filter((s) => s.group === name) }));
    const loose = filtered.filter((s) => !s.group);
    if (loose.length || out.length === 0 || dragSite) out.push({ name: "", sites: loose });
    return filter ? out.filter((g) => g.sites.length) : out;
  });

  let collapsed = $state<Record<string, boolean>>(loadLayout("groupsCollapsed", {}));
  const isCollapsed = (g: string) => !filter && !!collapsed[g];

  function toggleGroup(g: string) {
    if (collapsed[g]) delete collapsed[g];
    else collapsed[g] = true;
    saveLayout("groupsCollapsed", collapsed);
  }

  async function setGroup(ids: string[], group: string) {
    try {
      await api.siteSetGroup(ids, group);
      ui.sites = await api.sitesList();
    } catch (e) {
      log(`${t("Gruppe")}: ${te(e)}`, "error");
    }
  }

  async function askGroup(s: Site) {
    const name = await promptText(t("Gruppe (bestehend oder neu)"), s.group);
    if (name !== null && name !== s.group) await setGroup([s.id], name);
  }

  function groupContext(e: MouseEvent, g: string) {
    e.preventDefault();
    if (!g) {
      siteMenu = { x: e.clientX, y: e.clientY, items: [{ label: t("Neue Gruppe …"), action: newGroup }] };
      return;
    }
    const ids = ui.sites.filter((s) => s.group === g).map((s) => s.id);
    siteMenu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        {
          label: t("Gruppe umbenennen"),
          action: async () => {
            const name = await promptText(t("Neuer Gruppenname"), g);
            if (!name || name === g) return;
            if (collapsed[g]) collapsed[name] = true;
            if (ids.length) await setGroup(ids, name);
            else emptyGroups = [...emptyGroups.filter((x) => x !== g), name];
            saveLayout("emptyGroups", emptyGroups);
          },
        },
        { label: t("Neue Gruppe …"), action: newGroup },
        { separator: true },
        ids.length
          ? { label: t("Gruppe auflösen"), hint: t("Sites bleiben erhalten"), danger: true, action: () => setGroup(ids, "") }
          : { label: t("Gruppe löschen"), danger: true, action: () => removeEmptyGroup(g) },
      ],
    };
  }

  let dragSite = $state<Site | null>(null);
  let dropGroup = $state<string | null>(null);
  let ghost = $state<{ x: number; y: number } | null>(null);
  let dragMoved = false;

  function startSiteDrag(e: PointerEvent, s: Site) {
    if (e.button !== 0) return;
    const sx = e.clientX;
    const sy = e.clientY;
    dragMoved = false;
    const move = (ev: PointerEvent) => {
      if (!dragMoved && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 6) return;
      if (!dragMoved) document.body.classList.add("site-dragging");
      dragMoved = true;
      dragSite = s;
      ghost = { x: ev.clientX, y: ev.clientY };
      const el = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-group]");
      dropGroup = el ? (el.dataset.group ?? "") : null;
    };
    const up = async () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      document.body.classList.remove("site-dragging");
      const site = dragSite;
      const target = dropGroup;
      dragSite = null;
      dropGroup = null;
      ghost = null;
      setTimeout(() => (dragMoved = false));
      if (!site || target === null) return;
      if (target === NEW_GROUP) {
        const name = await promptText(t("Name der neuen Gruppe"));
        if (name) await setGroup([site.id], name);
      } else if (target !== site.group) {
        if (collapsed[target]) toggleGroup(target);
        await setGroup([site.id], target);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
</script>

<div class="app" class:sidebar-left={prefs.sidebar === "left"}>
  <main>
    {#if update}
      <div class="update-bar">
        <span>{t("Neue Version {latest} verfügbar (installiert: {current})", { latest: update.latest, current: update.current })}</span>
        <div class="spacer"></div>
        <button class="primary" onclick={() => update && openUrl(update.url)}>{t("Herunterladen")}</button>
        <button class="icon" title={t("Schliessen")} onclick={() => (update = null)}>×</button>
      </div>
    {/if}
    <nav class="tabs">
      <button class="app-prefs" class:active={showPrefs} title={t("Programm-Einstellungen: Sprache, Design und Layout")} onclick={() => (showPrefs = true)}>
        <Icon name="settings" size={17} />
      </button>
      {#if master.configured}
        <button
          class="app-prefs"
          class:unlocked={master.unlocked}
          title={master.unlocked ? t("Passwörter sind freigegeben – klicken zum Sperren") : t("Passwörter sind gesperrt – klicken zum Entsperren")}
          onclick={() => (master.unlocked ? lockMaster() : ensureUnlocked())}
        >
          <Icon name={master.unlocked ? "unlock" : "lock"} size={17} />
        </button>
      {/if}
      {#each TABS as [id, label]}
        <button class:active={ui.tab === id} onclick={() => (ui.tab = id)}>
          {t(label)}{id === "editor" && editorState.files.length ? ` (${editorState.files.length})` : ""}
        </button>
      {/each}
      {#if site}
        <button onclick={() => openSettings(site.id)}>{t("Einstellungen")}</button>
      {/if}
      <div class="spacer"></div>
      {#if site && siteUrl}
        <button class="link" onclick={() => openUrl(siteUrl)}>{siteUrl}</button>
      {/if}
    </nav>

    <section class="view">
      <div class="view-pane" class:hidden={ui.tab !== "editor"}>
        <EditorView />
      </div>
      {#if ui.tab !== "editor"}
        {#if !site}
          <div class="hint center">
            {t("Noch keine Sites. Mit «Import Dreamweaver» oder «Import Ordner …» starten.")}
          </div>
        {:else}
          <div class="view-pane">
            {#if ui.tab === "files"}
              <FilesView {site} />
            {:else if ui.tab === "git"}
              <GitView {site} />
            {:else if ui.tab === "logins"}
              <LoginsView {site} />
            {/if}
          </div>
        {/if}
      {/if}
    </section>

    <TransferPanel />

    {#if showLog}
      <div class="log">
        {#each ui.log as l}
          <div class="log-line {l.level}"><span class="muted">{l.time}</span> {l.msg}</div>
        {/each}
      </div>
    {/if}

    <footer class="statusbar">
      <button class="link" onclick={() => (showLog = !showLog)}>{t("Protokoll")} {showLog ? "▾" : "▴"}</button>
      {#if ui.progress}
        <div class="progress">
          {#if ui.progress.pct != null}<div class="bar" style="width: {ui.progress.pct}%"></div>{/if}
          <span>{ui.progress.text}</span>
        </div>
      {:else if lastLog}
        <span class="log-line {lastLog.level}">{lastLog.msg}</span>
      {/if}
    </footer>
  </main>

  <div class="gutter" role="separator" aria-orientation="vertical" onpointerdown={resizeSidebar}></div>

  <aside class="sidebar" style="width: {sidebarWidth}px">
    <div class="brand">
      <span>Dynooh SiteManager</span>
      {#if version}<span class="app-version" title={t("Version {version}", { version })}>v{version}</span>{/if}
      <div class="spacer"></div>
      <button class="icon-btn" title={t("Neue Site anlegen")} onclick={newSite}><Icon name="sitePlus" /></button>
      <button class="icon-btn" title={t("Neue Gruppe anlegen")} onclick={newGroup}><Icon name="folderPlus" /></button>
    </div>
    <input class="search" placeholder={t("Site suchen …")} bind:value={filter} />
    <ul class="site-list">
      {#each grouped as g (g.name)}
        {#if g.name || grouped.length > 1}
          <li
            class="group-head"
            class:drop-target={dragSite && dropGroup === g.name}
            data-group={g.name}
            oncontextmenu={(e) => groupContext(e, g.name)}
          >
            <button onclick={() => toggleGroup(g.name)}>
              <span class="twist">{isCollapsed(g.name) ? "▸" : "▾"}</span>
              <span class="group-name">{g.name || t("Ohne Gruppe")}</span>
              <span class="muted">{g.sites.length}</span>
            </button>
          </li>
        {/if}
        {#if !isCollapsed(g.name)}
          {#each g.sites as s (s.id)}
            <li data-group={g.name} class:grouped={!!g.name} class:drop-into={dragSite && dropGroup === g.name}>
              <button
                class:active={s.id === ui.currentId}
                class:dragging-site={dragSite?.id === s.id}
                onclick={() => !dragMoved && selectSite(s.id)}
                oncontextmenu={(e) => siteContext(e, s)}
                onpointerdown={(e) => startSiteDrag(e, s)}
              >
                <span class="site-name">{s.name}</span>
                <span class="site-host">
                  {s.host || t("kein Host")}{s.port && s.port !== 21 ? `:${s.port}` : ""}
                  {#if !s.user || !s.hasPassword}<span class="warn-dot" title={t("FTP-Login unvollständig")}>●</span>{/if}
                </span>
              </button>
            </li>
          {/each}
        {/if}
      {/each}
      {#if dragSite}
        <li class="group-head new-group" class:drop-target={dropGroup === NEW_GROUP} data-group={NEW_GROUP}>
          <button tabindex="-1"><span class="twist">＋</span><span class="group-name">{t("Neue Gruppe")}</span></button>
        </li>
      {/if}
    </ul>
    <div class="sidebar-actions">
      <button class="icon-btn" title={t("Dreamweaver importieren: übernimmt die in Dreamweaver eingerichteten Sites")} onclick={importDw}>
        <Icon name="dreamweaver" />
      </button>
      <button class="icon-btn" title={t("Ordner importieren: legt für jeden Unterordner des gewählten Ordners eine Site an")} onclick={importFolders}>
        <Icon name="folderImport" />
      </button>
      <span class="divider"></span>
      <button
        class="icon-btn"
        title={t("Sites exportieren: verschlüsselte Datei mit Zugängen und Passwörtern")}
        onclick={() => (showExport = true)}
        disabled={!ui.sites.length}
      >
        <Icon name="exportFile" />
      </button>
      <button class="icon-btn" title={t("Export importieren: Sites aus einer verschlüsselten Exportdatei laden")} onclick={importExport}>
        <Icon name="importFile" />
      </button>
    </div>
  </aside>
</div>

<SettingsCard />

{#if showPrefs}
  <PrefsDialog onclose={() => (showPrefs = false)} />
{/if}

{#if showExport}
  <ExportDialog onclose={() => (showExport = false)} />
{/if}

{#if master.dialog}
  {#key master.dialog}
    <MasterDialog mode={master.dialog} />
  {/key}
{/if}

{#if dragSite && ghost}
  <div class="site-ghost" style="left: {ghost.x + 14}px; top: {ghost.y + 10}px">
    {dragSite.name}
    <span class="muted">
      → {dropGroup === null ? "…" : dropGroup === NEW_GROUP ? t("neue Gruppe") : dropGroup || t("ohne Gruppe")}
    </span>
  </div>
{/if}

{#if siteMenu}
  <ContextMenu x={siteMenu.x} y={siteMenu.y} items={siteMenu.items} onclose={() => (siteMenu = null)} />
{/if}

{#if ui.prompt}
  <div class="modal-backdrop prompt-backdrop">
    <form
      class="modal"
      onsubmit={(e) => {
        e.preventDefault();
        closePrompt(ui.prompt?.value ?? null);
      }}
    >
      <label>
        {ui.prompt.text}
        {#if ui.prompt.password}
          <input type="password" autocomplete="off" bind:this={promptInput} bind:value={ui.prompt.value} />
        {:else}
          <input bind:this={promptInput} bind:value={ui.prompt.value} />
        {/if}
      </label>
      <div class="row">
        <div class="spacer"></div>
        <button type="button" onclick={() => closePrompt(null)}>{t("Abbrechen")}</button>
        <button class="primary" type="submit">OK</button>
      </div>
    </form>
  </div>
{/if}
