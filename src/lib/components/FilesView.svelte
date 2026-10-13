<script lang="ts">
  import { untrack } from "svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, joinRel, webUrlFor, type Side, type Site } from "$lib/api";
  import { isBinary, log, openFile, promptText } from "$lib/state.svelte";
  import { clamp, drag, loadLayout, saveLayout } from "$lib/drag";
  import { saveSession, session } from "$lib/session";
  import { waitJob } from "$lib/jobs.svelte";
  import { t, te } from "$lib/i18n.svelte";
  import { prefs } from "$lib/prefs.svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import FilePane, { type Sel } from "./FilePane.svelte";
  import SyncView from "./SyncView.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let { site }: { site: Site } = $props();

  const pane = $state({
    local: { exp: {} as Record<string, boolean>, sel: [] as Sel[], reload: 0, reveal: { rel: "", seq: 0 } },
    remote: { exp: {} as Record<string, boolean>, sel: [] as Sel[], reload: 0, reveal: { rel: "", seq: 0 } },
  });

  /** Folder being synchronised; null shows the file panes. */
  let syncRel = $state<string | null>(null);

  function expandTo(side: Side, dir: string) {
    const parts = dir ? dir.split("/") : [];
    for (let i = 1; i <= parts.length; i++) {
      const rel = parts.slice(0, i).join("/");
      if (!pane[side].exp[rel]) toggle(side, rel, true);
    }
  }

  /** Opens and selects the folder a transfer writes into, so it shows in the path field. */
  function revealDir(side: Side, dir: string) {
    expandTo(side, dir);
    pane[side].sel = dir ? [{ rel: dir, isDir: true }] : [];
    pane[side].reveal = { rel: dir, seq: pane[side].reveal.seq + 1 };
  }

  /** Selects the transferred items in the target pane; they appear as soon as they arrive. */
  function revealItems(side: Side, items: Sel[]) {
    if (!items.length) return;
    expandTo(side, parentOf(items[0].rel));
    pane[side].sel = items.map((s) => ({ rel: s.rel, isDir: s.isDir }));
    pane[side].reveal = { rel: items[0].rel, seq: pane[side].reveal.seq + 1 };
  }
  let clipboard = $state<{ siteId: string; side: Side; items: Sel[] } | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let linked = $state(session.linked);
  let busy = $state(false);

  let panesEl = $state<HTMLDivElement | null>(null);
  let split = $state(loadLayout("split", 50));
  const order = $derived<Side[]>(
    prefs.panes === "remote" ? ["remote"] : prefs.panes === "local" ? ["local"] : prefs.serverSide === "right" ? ["local", "remote"] : ["remote", "local"],
  );
  const columns = $derived(order.length === 1 ? "minmax(0, 1fr)" : `minmax(0, ${split}fr) 5px minmax(0, ${100 - split}fr)`);

  function resizeSplit(e: PointerEvent) {
    if (!panesEl) return;
    const rect = panesEl.getBoundingClientRect();
    const start = split;
    drag(
      e,
      (dx) => (split = clamp(start + (dx / rect.width) * 100, 15, 85)),
      () => saveLayout("split", split),
    );
  }

  let paneSiteId = $state("");

  $effect(() => {
    const id = site.id;
    untrack(() => {
      const tr = session.trees[id];
      const toExp = (list?: string[]) => Object.fromEntries((list ?? []).map((k) => [k, true]));
      pane.local.exp = toExp(tr?.local);
      pane.remote.exp = toExp(tr?.remote);
      pane.local.sel = tr?.selLocal ?? [];
      pane.remote.sel = tr?.selRemote ?? [];
      paneSiteId = id;
      syncRel = null;
    });
  });

  $effect(() => {
    const tr = {
      local: Object.keys(pane.local.exp),
      remote: Object.keys(pane.remote.exp),
      selLocal: pane.local.sel.map((s) => ({ rel: s.rel, isDir: s.isDir })),
      selRemote: pane.remote.sel.map((s) => ({ rel: s.rel, isDir: s.isDir })),
    };
    session.linked = linked;
    if (paneSiteId && paneSiteId === site.id) session.trees[paneSiteId] = tr;
    saveSession();
  });

  const parentOf = (rel: string) => (rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "");
  const baseName = (rel: string) => rel.split("/").pop() ?? rel;
  const sideName = (side: Side) => (side === "local" ? t("lokal") : t("Server"));

  async function run(label: string, fn: () => Promise<unknown>) {
    busy = true;
    try {
      await fn();
    } catch (e) {
      log(`${label}: ${te(e)}`, "error");
    } finally {
      busy = false;
    }
  }

  function upload(items = pane.local.sel) {
    if (!items.length) return;
    api.upload(site.id, items.map((s) => s.rel)).catch((e) => log(`${t("Hochladen")}: ${te(e)}`, "error"));
    revealItems("remote", items);
  }

  function download(items = pane.remote.sel) {
    if (!items.length) return;
    api.download(site.id, items.map((s) => s.rel)).catch((e) => log(`${t("Herunterladen")}: ${te(e)}`, "error"));
    revealItems("local", items);
  }

  async function openLocal(rel: string) {
    if (isBinary(rel)) await revealItemInDir(await api.localPath(site.id, rel));
    else openFile(site.id, rel);
  }

  const openRemote = (rel: string) =>
    run(t("Öffnen"), async () => {
      const job = await waitJob(await api.download(site.id, [rel]));
      if (job.state === "done") await openLocal(rel);
    });

  const open = (side: Side, rel: string) => (side === "local" ? openLocal(rel) : openRemote(rel));

  async function mkdir(side: Side, dir: string) {
    const name = await promptText(side === "local" ? t("Name des neuen Ordners (lokal)") : t("Name des neuen Ordners (Server)"));
    if (!name) return;
    await run(t("Ordner anlegen"), async () => {
      const rel = joinRel(dir, name);
      if (side === "local") await api.localMkdir(site.id, rel);
      else await api.remoteMkdir(site.id, rel);
      pane[side].reload++;
    });
  }

  async function rename(side: Side, item: Sel) {
    const name = await promptText(t("Neuer Name"), baseName(item.rel));
    if (!name || name === baseName(item.rel)) return;
    await run(t("Umbenennen"), async () => {
      const to = joinRel(parentOf(item.rel), name);
      if (side === "local") await api.localRename(site.id, item.rel, to);
      else await api.remoteRename(site.id, item.rel, to);
      pane[side].sel = [];
      pane[side].reload++;
    });
  }

  function copy(side: Side, items: Sel[]) {
    if (items.length === 0) return;
    clipboard = { siteId: site.id, side, items: items.map((s) => ({ ...s })) };
    log(t("{n} Element(e) kopiert ({side})", { n: items.length, side: sideName(side) }));
  }

  async function paste(side: Side, dest: string) {
    const clip = clipboard;
    if (!clip || clip.siteId !== site.id) return;
    try {
      await api.copyItems(site.id, clip.side, clip.items, side, dest);
      if (clip.side === side) revealDir(side, dest);
      else revealItems(side, clip.items.map((s) => ({ rel: joinRel(dest, baseName(s.rel)), isDir: s.isDir })));
    } catch (e) {
      log(`${t("Einfügen")}: ${te(e)}`, "error");
    }
  }

  async function remove(side: Side, selection: Sel[]) {
    const items = selection.filter((s) => !selection.some((o) => o.isDir && o !== s && s.rel.startsWith(o.rel + "/")));
    if (items.length === 0) return;
    const list =
      items.slice(0, 20).map((s) => s.rel + (s.isDir ? "/" : "")).join("\n") +
      (items.length > 20 ? "\n" + t("… und {n} weitere", { n: items.length - 20 }) : "");
    const ok =
      side === "local"
        ? await confirm(`${t("{n} Element(e) lokal in den Papierkorb verschieben?", { n: items.length })}\n\n${list}`, { title: t("Lokal löschen"), kind: "warning" })
        : await confirm(`${t("{n} Element(e) auf dem Server endgültig löschen?", { n: items.length })}\n\n${list}`, { title: t("Auf dem Server löschen"), kind: "warning" });
    if (!ok) return;
    await run(t("Löschen"), async () => {
      if (side === "local") {
        await api.localDelete(site.id, items.map((s) => s.rel));
        log(t("{n} Element(e) lokal in den Papierkorb verschoben", { n: items.length }), "ok");
      } else {
        for (const s of items) {
          await api.remoteDelete(site.id, s.rel, s.isDir);
          log(t("Gelöscht (Server): {path}", { path: s.rel }), "ok");
        }
      }
      const parent = parentOf(items[0].rel);
      pane[side].sel = parent ? [{ rel: parent, isDir: true }] : [];
      pane[side].reload++;
    });
  }

  async function reveal(rel: string) {
    try {
      await revealItemInDir(await api.revealPath(site.id, rel));
    } catch (e) {
      log(`Explorer: ${te(e)}`, "error");
    }
  }

  function showMenu(side: Side, x: number, y: number, target: Sel | null) {
    const sel = pane[side].sel;
    const single = target && sel.length <= 1 ? target : null;
    const dest = destFor(target && sel.length <= 1 ? target : sel[0] ?? null);
    const canPaste = !!clipboard && clipboard.siteId === site.id;
    const pasteHint = canPaste
      ? clipboard!.side === side
        ? t("Kopie")
        : clipboard!.side === "local"
          ? t("hochladen")
          : t("herunterladen")
      : "";
    const url = webUrlFor(site, target?.rel ?? "");

    const items: MenuItem[] = [];
    if (single && !single.isDir) items.push({ label: t("Öffnen"), hint: "Enter", action: () => open(side, single.rel) });
    if (single?.isDir)
      items.push({
        label: pane[side].exp[single.rel] ? t("Zuklappen") : t("Aufklappen"),
        hint: "Enter",
        action: () => toggle(side, single.rel, !pane[side].exp[single.rel]),
      });
    items.push({ label: t("Im Explorer zeigen"), action: () => reveal(target?.rel ?? "") });
    if (url) items.push({ label: t("Im Browser öffnen"), action: () => openUrl(url) });
    items.push({ separator: true });
    if (side === "local") items.push({ label: t("Hochladen"), disabled: sel.length === 0, action: () => upload(sel) });
    else items.push({ label: t("Herunterladen"), disabled: sel.length === 0, action: () => download(sel) });
    items.push({
      label: dest ? t("Ordner synchronisieren") : t("Gesamte Site synchronisieren"),
      hint: dest ? "/" + dest : "",
      action: () => (syncRel = dest),
    });
    items.push({ separator: true });
    items.push({ label: t("Kopieren"), hint: "Ctrl+C", disabled: sel.length === 0, action: () => copy(side, sel) });
    items.push({
      label: canPaste ? `${t("Einfügen")} (${clipboard!.items.length}, ${pasteHint})` : t("Einfügen"),
      hint: "Ctrl+V",
      disabled: !canPaste,
      action: () => paste(side, dest),
    });
    items.push({ separator: true });
    items.push({ label: t("Neuer Ordner"), action: () => mkdir(side, dest) });
    items.push({ label: t("Umbenennen"), hint: "F2", disabled: !single, action: () => single && rename(side, single) });
    items.push({
      label: side === "local" ? t("Löschen (Papierkorb)") : t("Löschen"),
      hint: t("Entf"),
      danger: true,
      disabled: sel.length === 0,
      action: () => remove(side, sel),
    });
    items.push({ separator: true });
    items.push({ label: t("Aktualisieren"), action: () => pane[side].reload++ });
    menu = { x, y, items };
  }

  function key(side: Side, e: KeyboardEvent) {
    const sel = pane[side].sel;
    if (e.ctrlKey && e.key.toLowerCase() === "c") {
      e.preventDefault();
      copy(side, sel);
    } else if (e.ctrlKey && e.key.toLowerCase() === "v") {
      e.preventDefault();
      paste(side, destFor(sel[0] ?? null));
    } else if (e.key === "F2" && sel.length === 1) {
      e.preventDefault();
      rename(side, sel[0]);
    } else if (e.key === "Delete") {
      e.preventDefault();
      remove(side, sel);
    } else if (e.key === "F5") {
      e.preventDefault();
      pane[side].reload++;
    }
  }

  function destFor(target: Sel | null): string {
    if (!target) return "";
    return target.isDir ? target.rel : parentOf(target.rel);
  }

  /** Folder for the toolbar sync button: the selected folder (or its parent) of the server pane, else the local one. */
  const syncTarget = $derived(destFor(pane.remote.sel[0] ?? pane.local.sel[0] ?? null));

  let dropTarget = $state<{ side: Side; dir: string } | null>(null);

  function dropTargetAt(pos: { x: number; y: number }): { side: Side; dir: string } | null {
    const el = document.elementFromPoint(pos.x / devicePixelRatio, pos.y / devicePixelRatio);
    const paneEl = el?.closest<HTMLElement>("[data-side]");
    if (!el || !paneEl || !panesEl?.contains(paneEl)) return null;
    const row = el.closest<HTMLElement>("tr[data-rel]");
    const rel = row?.dataset.rel ?? "";
    return { side: paneEl.dataset.side as Side, dir: !row ? "" : row.dataset.dir === "1" ? rel : parentOf(rel) };
  }

  async function dropFiles(target: { side: Side; dir: string }, paths: string[]) {
    const names = paths.map((p) => p.split(/[\\/]/).pop()).join("\n");
    const where = `${sideName(target.side)}: /${target.dir}`;
    const question =
      target.side === "local"
        ? t("{n} Element(e) nach {where} kopieren?", { n: paths.length, where })
        : t("{n} Element(e) nach {where} hochladen?", { n: paths.length, where });
    if (!(await confirm(`${question}\n${t("Gleichnamige Dateien werden überschrieben.")}\n\n${names}`, { title: t("Ablegen"), kind: "info" })))
      return;
    try {
      await api.dropPaths(site.id, paths, target.side, target.dir);
      revealItems(target.side, paths.map((p) => ({ rel: joinRel(target.dir, p.split(/[\\/]/).pop() ?? p), isDir: false })));
    } catch (e) {
      log(`${t("Ablegen")}: ${te(e)}`, "error");
    }
  }

  $effect(() => {
    const un = getCurrentWebview().onDragDropEvent(({ payload: p }) => {
      if (syncRel !== null) return;
      if (p.type === "enter" || p.type === "over") dropTarget = dropTargetAt(p.position);
      else if (p.type === "leave") dropTarget = null;
      else if (p.type === "drop") {
        const target = dropTargetAt(p.position);
        dropTarget = null;
        if (target && p.paths.length) dropFiles(target, p.paths);
      }
    });
    return () => {
      un.then((f) => f());
    };
  });

  function toggle(side: Side, rel: string, open: boolean) {
    for (const p of linked ? [pane.local, pane.remote] : [pane[side]]) {
      if (open) p.exp[rel] = true;
      else delete p.exp[rel];
    }
  }

  function collapseAll(side: Side) {
    for (const s of linked ? (["local", "remote"] as Side[]) : [side]) pane[s].exp = {};
  }
</script>

{#if syncRel !== null}
  <SyncView {site} rel={syncRel} onclose={() => (syncRel = null)} />
{:else}
  <div class="files-view">
    <div class="toolbar">
      {#if prefs.panes === "local"}
        <button onclick={() => upload()} disabled={pane.local.sel.length === 0} title={t("Auswahl lokal → Server")}>{t("Hochladen")}</button>
      {:else if prefs.panes === "remote"}
        <button onclick={() => download()} disabled={pane.remote.sel.length === 0} title={t("Auswahl Server → lokal")}>{t("Herunterladen")}</button>
      {:else if prefs.serverSide === "right"}
        <button onclick={() => upload()} disabled={pane.local.sel.length === 0} title={t("Auswahl lokal → Server")}>{t("Hochladen")} →</button>
        <button onclick={() => download()} disabled={pane.remote.sel.length === 0} title={t("Auswahl Server → lokal")}>← {t("Herunterladen")}</button>
      {:else}
        <button onclick={() => upload()} disabled={pane.local.sel.length === 0} title={t("Auswahl lokal → Server")}>← {t("Hochladen")}</button>
        <button onclick={() => download()} disabled={pane.remote.sel.length === 0} title={t("Auswahl Server → lokal")}>{t("Herunterladen")} →</button>
      {/if}
      <button onclick={() => (syncRel = syncTarget)} title={t("Lokale und Server-Dateien vergleichen und abgleichen")}>
        ⇅ {t("Synchronisieren")}{syncTarget ? ` /${syncTarget}` : ""}
      </button>
      <span class="divider"></span>
      <label class="check"><input type="checkbox" bind:checked={linked} /> {t("Ordner koppeln")}</label>
      {#if clipboard && clipboard.siteId === site.id}
        <span class="muted">{t("Zwischenablage: {n} Element(e) ({side})", { n: clipboard.items.length, side: sideName(clipboard.side) })}</span>
      {/if}
      <div class="spacer"></div>
      <span class="muted">{t("Rechtsklick für weitere Aktionen")}</span>
    </div>
    <div class="panes" bind:this={panesEl} style="grid-template-columns: {columns}">
      {#each order as side, i (side)}
        {#if i === 1}
          <div class="gutter" role="separator" aria-orientation="vertical" onpointerdown={resizeSplit}></div>
        {/if}
        <FilePane
          {site}
          {side}
          expanded={pane[side].exp}
          bind:selected={pane[side].sel}
          reload={pane[side].reload}
          dropDir={dropTarget?.side === side ? dropTarget.dir : null}
          reveal={pane[side].reveal}
          ontoggle={(rel, open) => toggle(side, rel, open)}
          oncollapseall={() => collapseAll(side)}
          onopen={(rel) => (side === "local" ? openLocal(rel) : openRemote(rel))}
          oncontext={(x, y, target) => showMenu(side, x, y, target)}
          onkey={(e) => key(side, e)}
        />
      {/each}
    </div>
  </div>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}
