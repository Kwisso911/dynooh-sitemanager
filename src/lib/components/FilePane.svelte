<script lang="ts" module>
  export interface Sel {
    rel: string;
    isDir: boolean;
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, joinRel, type Entry, type FsChanged, type Site } from "$lib/api";
  import { fmtDate, fmtSize } from "$lib/state.svelte";
  import { drag, loadLayout, saveLayout } from "$lib/drag";
  import { saveSession, session } from "$lib/session";
  import { t, te } from "$lib/i18n.svelte";

  interface Props {
    site: Site;
    side: "local" | "remote";
    expanded: Record<string, boolean>;
    selected: Sel[];
    reload: number;
    dropDir?: string | null;
    reveal?: { rel: string; seq: number };
    ontoggle: (rel: string, open: boolean) => void;
    oncollapseall?: () => void;
    onopen?: (rel: string, entry: Entry) => void;
    oncontext?: (x: number, y: number, target: Sel | null) => void;
    onkey?: (e: KeyboardEvent) => void;
    ondragitems?: (ev: PointerEvent, items: Sel[]) => void;
  }

  let {
    site,
    side,
    expanded,
    selected = $bindable([]),
    reload,
    dropDir = null,
    reveal,
    ontoggle,
    oncollapseall,
    onopen,
    oncontext,
    onkey,
    ondragitems,
  }: Props = $props();

  type Row =
    | { kind: "entry"; e: Entry; rel: string; depth: number }
    | { kind: "info"; text: string; depth: number; error?: boolean };

  let children = $state<Record<string, Entry[]>>({});
  let loading = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string>>({});
  let anchor = -1;
  let bodyEl = $state<HTMLDivElement | null>(null);
  let cols = $state<number[]>(loadLayout("filecols", [280, 80, 130]));

  function resizeCol(e: PointerEvent, i: number) {
    e.stopPropagation();
    const start = cols[i];
    drag(
      e,
      (dx) => (cols[i] = Math.max(50, start + dx)),
      () => saveLayout("filecols", cols),
    );
  }

  async function load(dir: string) {
    loading[dir] = true;
    delete errors[dir];
    try {
      children[dir] = side === "local" ? await api.localList(site.id, dir) : await api.remoteList(site.id, dir);
    } catch (e) {
      errors[dir] = te(e);
      children[dir] = [];
    } finally {
      delete loading[dir];
    }
  }

  let changed = new Set<string>();
  let changeTimer: ReturnType<typeof setTimeout> | null = null;

  function flushChanged() {
    changeTimer = null;
    const dirs = new Set<string>();
    for (let d of changed) {
      while (d && !(d in children)) d = d.includes("/") ? d.slice(0, d.lastIndexOf("/")) : "";
      if (d in children) dirs.add(d);
    }
    changed.clear();
    for (const d of dirs) {
      if (loading[d]) changed.add(d);
      else load(d);
    }
    if (changed.size) changeTimer = setTimeout(flushChanged, 500);
  }

  $effect(() => {
    const id = site.id;
    const un = listen<FsChanged>("fs-changed", ({ payload: p }) => {
      if (p.siteId !== id || p.side !== side) return;
      changed.add(p.dir);
      changeTimer ??= setTimeout(flushChanged, side === "remote" ? 700 : 250);
    });
    return () => {
      un.then((f) => f());
      if (changeTimer) clearTimeout(changeTimer);
      changeTimer = null;
      changed.clear();
    };
  });

  let rootKey = "";

  /** Reloads all open folders in place, so the view and scroll position stay where they are. */
  function refreshOpen() {
    const open = Object.keys(children).filter((d) => d === "" || expanded[d]);
    for (const d of Object.keys(children)) if (!open.includes(d)) delete children[d];
    for (const d of open.length ? open : [""]) load(d);
  }

  $effect(() => {
    const key = [site.id, site.host, site.remotePath, site.localPath].join("|");
    void reload;
    untrack(() => {
      if (key !== rootKey) {
        rootKey = key;
        children = {};
        errors = {};
        restoreTop = session.scroll[scrollKey()] || null;
        load("");
      } else refreshOpen();
    });
  });

  let restoreTop = $state<number | null>(null);
  const scrollKey = () => `${site.id}:${side}`;

  $effect(() => {
    if (restoreTop === null || !bodyEl || tree.pending.length || Object.keys(loading).length) return;
    const top = restoreTop;
    untrack(() => {
      bodyEl!.scrollTop = top;
      restoreTop = null;
    });
  });

  function rememberScroll() {
    if (restoreTop !== null || !bodyEl) return;
    session.scroll[scrollKey()] = Math.round(bodyEl.scrollTop);
    saveSession();
  }

  const tree = $derived.by(() => {
    const rows: Row[] = [];
    const pending: string[] = [];
    const walk = (dir: string, depth: number) => {
      const list = children[dir];
      if (!list) {
        pending.push(dir);
        rows.push({ kind: "info", text: t("Lade …"), depth });
        return;
      }
      if (errors[dir]) rows.push({ kind: "info", text: errors[dir], depth, error: true });
      else if (list.length === 0) rows.push({ kind: "info", text: dir ? t("leer") : t("Ordner ist leer"), depth });
      for (const e of list) {
        const rel = joinRel(dir, e.name);
        rows.push({ kind: "entry", e, rel, depth });
        if (e.isDir && expanded[rel]) walk(rel, depth + 1);
      }
    };
    walk("", 0);
    return { rows, pending };
  });

  const entryRows = $derived(tree.rows.filter((r): r is Extract<Row, { kind: "entry" }> => r.kind === "entry"));

  $effect(() => {
    for (const dir of tree.pending) {
      if (!loading[dir]) untrack(() => load(dir));
    }
  });

  function isSelected(rel: string) {
    return selected.some((s) => s.rel === rel);
  }

  function selectIndex(i: number) {
    const r = entryRows[i];
    if (!r) return;
    selected = [{ rel: r.rel, isDir: r.e.isDir }];
    anchor = i;
    bodyEl?.querySelector(`[data-rel="${CSS.escape(r.rel)}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function click(ev: MouseEvent, rel: string) {
    const i = entryRows.findIndex((r) => r.rel === rel);
    const r = entryRows[i];
    const item = { rel, isDir: r.e.isDir };
    if (ev.shiftKey && anchor >= 0) {
      const [a, b] = anchor < i ? [anchor, i] : [i, anchor];
      selected = entryRows.slice(a, b + 1).map((x) => ({ rel: x.rel, isDir: x.e.isDir }));
    } else if (ev.ctrlKey || ev.metaKey) {
      selected = isSelected(rel) ? selected.filter((s) => s.rel !== rel) : [...selected, item];
      anchor = i;
    } else {
      selected = [item];
      anchor = i;
    }
  }

  /** Drags the whole selection when the row is part of it, otherwise only the row. */
  function pointerdown(ev: PointerEvent, rel: string, isDir: boolean) {
    if (ev.button !== 0 || !ondragitems) return;
    ondragitems(ev, isSelected(rel) ? selected.map((s) => ({ ...s })) : [{ rel, isDir }]);
  }

  function activate(rel: string, e: Entry) {
    if (e.isDir) ontoggle(rel, !expanded[rel]);
    else onopen?.(rel, e);
  }

  function twist(ev: MouseEvent, rel: string) {
    ev.stopPropagation();
    ontoggle(rel, !expanded[rel]);
  }

  function context(ev: MouseEvent, rel: string | null) {
    ev.preventDefault();
    ev.stopPropagation();
    let target: Sel | null = null;
    if (rel === null) {
      selected = [];
    } else {
      const i = entryRows.findIndex((r) => r.rel === rel);
      target = { rel, isDir: entryRows[i].e.isDir };
      if (!isSelected(rel)) {
        selected = [target];
        anchor = i;
      }
    }
    oncontext?.(ev.clientX, ev.clientY, target);
  }

  function keydown(ev: KeyboardEvent) {
    const cur = selected.length ? entryRows.findIndex((r) => r.rel === selected[selected.length - 1].rel) : -1;
    const row = cur >= 0 ? entryRows[cur] : null;
    if (ev.key === "ArrowDown") {
      ev.preventDefault();
      selectIndex(Math.min(entryRows.length - 1, cur + 1));
    } else if (ev.key === "ArrowUp") {
      ev.preventDefault();
      selectIndex(Math.max(0, cur - 1));
    } else if (ev.key === "ArrowRight" && row?.e.isDir) {
      ev.preventDefault();
      if (!expanded[row.rel]) ontoggle(row.rel, true);
      else selectIndex(cur + 1);
    } else if (ev.key === "ArrowLeft" && row) {
      ev.preventDefault();
      if (row.e.isDir && expanded[row.rel]) ontoggle(row.rel, false);
      else {
        const parent = row.rel.includes("/") ? row.rel.slice(0, row.rel.lastIndexOf("/")) : "";
        const pi = entryRows.findIndex((r) => r.rel === parent);
        if (pi >= 0) selectIndex(pi);
      }
    } else if (ev.key === "Enter" && row && selected.length === 1) {
      ev.preventDefault();
      activate(row.rel, row.e);
    } else if (ev.key === "a" && ev.ctrlKey) {
      ev.preventDefault();
      selected = entryRows.map((x) => ({ rel: x.rel, isDir: x.e.isDir }));
    } else {
      onkey?.(ev);
    }
  }

  const rootLabel = $derived(side === "local" ? site.localPath : `${site.host}${site.remotePath ? " : " + site.remotePath : ""}`);

  const parentOf = (rel: string) => (rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "");
  const normPath = (s: string) => s.replace(/\\/g, "/").replace(/\/{2,}/g, "/").replace(/^\/+/, "");

  let pathText = $state("/");
  let pathFocus = $state(false);
  let pathError = $state(false);
  let suggestions = $state<Entry[]>([]);
  let sugIndex = $state(-1);
  let sugToken = 0;
  let pendingScroll = $state<string | null>(null);
  let pathEl = $state<HTMLInputElement | null>(null);

  $effect(() => {
    const s = selected[selected.length - 1];
    const d = !s ? "" : s.isDir ? s.rel : parentOf(s.rel);
    if (!pathFocus) pathText = "/" + (d ? d + "/" : "");
  });

  $effect(() => {
    const kind = new Map(entryRows.map((r) => [r.rel, r.e.isDir]));
    if (selected.some((s) => kind.has(s.rel) && kind.get(s.rel) !== s.isDir)) {
      untrack(() => (selected = selected.map((s) => (kind.has(s.rel) ? { rel: s.rel, isDir: kind.get(s.rel)! } : s))));
    }
  });

  $effect(() => {
    if (!reveal?.seq) return;
    const rel = reveal.rel;
    untrack(() => {
      if (rel) pendingScroll = rel;
      else bodyEl?.scrollTo({ top: 0 });
    });
  });

  $effect(() => {
    const target = pendingScroll;
    if (target && entryRows.some((r) => r.rel === target)) {
      untrack(() => {
        bodyEl?.querySelector(`[data-rel="${CSS.escape(target)}"]`)?.scrollIntoView({ block: "center" });
        pendingScroll = null;
      });
    }
  });

  function splitPath(s: string) {
    const r = normPath(s);
    const i = r.lastIndexOf("/");
    return { dir: i < 0 ? "" : r.slice(0, i), prefix: i < 0 ? r : r.slice(i + 1) };
  }

  async function listDir(dir: string): Promise<Entry[]> {
    if (children[dir] && !errors[dir]) return children[dir];
    return side === "local" ? api.localList(site.id, dir) : api.remoteList(site.id, dir);
  }

  async function suggest() {
    const { dir, prefix } = splitPath(pathText);
    const token = ++sugToken;
    try {
      const list = await listDir(dir);
      if (token !== sugToken) return;
      const p = prefix.toLowerCase();
      suggestions = list.filter((e) => e.name.toLowerCase().startsWith(p)).slice(0, 60);
      sugIndex = -1;
    } catch {
      if (token === sugToken) suggestions = [];
    }
  }

  function accept(e: Entry) {
    const { dir } = splitPath(pathText);
    const rel = joinRel(dir, e.name);
    if (e.isDir) {
      pathText = `/${rel}/`;
      suggest();
    } else {
      pathText = `/${rel}`;
      go();
    }
  }

  async function go() {
    const rel = normPath(pathText).replace(/\/+$/, "");
    suggestions = [];
    if (!rel) {
      selected = [];
      bodyEl?.scrollTo({ top: 0 });
      bodyEl?.focus();
      return;
    }
    const name = rel.split("/").pop() ?? rel;
    let entry: Entry | undefined;
    try {
      entry = (await listDir(parentOf(rel))).find((e) => e.name === name);
    } catch {
      entry = undefined;
    }
    if (!entry) {
      pathError = true;
      setTimeout(() => (pathError = false), 1500);
      return;
    }
    const parts = rel.split("/");
    for (let i = 1; i < parts.length; i++) {
      const anc = parts.slice(0, i).join("/");
      if (!expanded[anc]) ontoggle(anc, true);
    }
    if (entry.isDir && !expanded[rel]) ontoggle(rel, true);
    selected = [{ rel, isDir: entry.isDir }];
    pendingScroll = rel;
    pathEl?.blur();
    bodyEl?.focus();
  }

  function pathKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" && suggestions.length) {
      e.preventDefault();
      sugIndex = (sugIndex + 1) % suggestions.length;
    } else if (e.key === "ArrowUp" && suggestions.length) {
      e.preventDefault();
      sugIndex = sugIndex <= 0 ? suggestions.length - 1 : sugIndex - 1;
    } else if (e.key === "Tab" && suggestions.length) {
      e.preventDefault();
      accept(suggestions[Math.max(0, sugIndex)]);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (sugIndex >= 0 && suggestions[sugIndex]) accept(suggestions[sugIndex]);
      else go();
    } else if (e.key === "Escape") {
      e.preventDefault();
      suggestions = [];
      pathEl?.blur();
    }
  }
</script>

<div class="pane" class:drop-active={dropDir === ""} data-side={side}>
  <div class="pane-head">
    <strong>{side === "local" ? t("Lokal") : t("Server")}</strong>
    <button class="icon" title={t("Aktualisieren (F5)")} onclick={refreshOpen}>⟳</button>
    <button class="icon" title={t("Alle zuklappen")} onclick={() => oncollapseall?.()}>⊟</button>
    <span class="root-label muted" title={rootLabel}>{rootLabel}</span>
    <div class="path-box">
      <input
        class="path-input"
        class:error={pathError}
        bind:this={pathEl}
        bind:value={pathText}
        spellcheck="false"
        autocomplete="off"
        title={t("Pfad eingeben: Tab vervollständigt, Enter springt hin")}
        onfocus={() => ((pathFocus = true), suggest())}
        onblur={() => ((pathFocus = false), (suggestions = []))}
        oninput={suggest}
        onkeydown={pathKey}
      />
      {#if pathFocus && suggestions.length}
        <ul class="path-suggest">
          {#each suggestions as s, i (s.name)}
            <li class:active={i === sugIndex}>
              <button type="button" tabindex="-1" onmousedown={(e) => (e.preventDefault(), accept(s))}>
                <span class="ico" class:dir={s.isDir}></span>{s.name}{s.isDir ? "/" : ""}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="pane-body" bind:this={bodyEl} tabindex="0" role="tree" oncontextmenu={(ev) => context(ev, null)} onkeydown={keydown} onscroll={rememberScroll}>
    <table class="files fixed" style="min-width: {cols.reduce((a, b) => a + b, 0)}px">
      <colgroup>
        {#each cols as w}<col style="width: {w}px" />{/each}
        <col />
      </colgroup>
      <thead>
        <tr>
          {#each [t("Name"), t("Grösse"), t("Geändert")] as label, i}
            <th class:num={i === 1}>
              {label}
              <span class="col-resize" role="separator" aria-orientation="vertical" onpointerdown={(e) => resizeCol(e, i)}></span>
            </th>
          {/each}
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each tree.rows as r, i (r.kind === "entry" ? r.rel : `info:${i}`)}
          {#if r.kind === "entry"}
            <tr
              data-rel={r.rel}
              data-dir={r.e.isDir ? "1" : "0"}
              class:drop-row={dropDir !== null && dropDir !== "" && r.rel === dropDir}
              class:selected={isSelected(r.rel)}
              class:excluded={site.exclude.includes(r.e.name)}
              onpointerdown={(ev) => pointerdown(ev, r.rel, r.e.isDir)}
              onclick={(ev) => click(ev, r.rel)}
              ondblclick={() => activate(r.rel, r.e)}
              oncontextmenu={(ev) => context(ev, r.rel)}
            >
              <td style="padding-left: {6 + r.depth * 16}px">
                {#if r.e.isDir}
                  <span class="twist" role="button" tabindex="-1" onclick={(ev) => twist(ev, r.rel)} ondblclick={(ev) => ev.stopPropagation()} onkeydown={() => {}}>
                    {expanded[r.rel] ? "▾" : "▸"}
                  </span>
                {:else}
                  <span class="twist"></span>
                {/if}
                <span class="ico" class:dir={r.e.isDir}></span>{r.e.name}{#if r.e.link}<span class="muted" title={t("Symbolischer Link")}> ↪</span>{/if}
              </td>
              <td class="num">{r.e.isDir ? "" : fmtSize(r.e.size)}</td>
              <td>{fmtDate(r.e.mtime)}</td>
              <td></td>
            </tr>
          {:else}
            <tr class="info-row">
              <td colspan="4" class:error={r.error} style="padding-left: {22 + r.depth * 16}px">{r.text}</td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  </div>
</div>
