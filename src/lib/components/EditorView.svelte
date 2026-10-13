<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import { editorState, log, ui, type OpenFile } from "$lib/state.svelte";
  import type { monaco as Monaco } from "$lib/monaco";
  import { saveSession, session } from "$lib/session";
  import { t, te } from "$lib/i18n.svelte";
  import { isDark, prefs } from "$lib/prefs.svelte";

  type M = typeof Monaco;
  interface Doc {
    model: Monaco.editor.ITextModel;
    saved: number;
    view: Monaco.editor.ICodeEditorViewState | null;
  }

  let host: HTMLDivElement;
  let monaco: M | null = null;
  let editor: Monaco.editor.IStandaloneCodeEditor | null = null;
  const docs = new Map<string, Doc>();
  let shownKey = "";
  let cursor = $state("");

  onMount(async () => {
    monaco = (await import("$lib/monaco")).monaco;
    editor = monaco.editor.create(host, {
      theme: isDark() ? "vs-dark" : "vs",
      automaticLayout: true,
      fontSize: 13,
      minimap: { enabled: true },
      wordWrap: "off",
      renderWhitespace: "selection",
      model: null,
    });
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => save(false));
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyU, () => save(true));
    editor.onDidChangeCursorPosition((e) => {
      cursor = t("Zeile {line}, Spalte {col}", { line: e.position.lineNumber, col: e.position.column });
      rememberView();
    });
    editor.onDidScrollChange(rememberView);
    show(editorState.active);
  });

  onDestroy(() => {
    editor?.dispose();
    docs.forEach((d) => d.model.dispose());
  });

  $effect(() => {
    void prefs.theme;
    void prefs.custom.bg;
    monaco?.editor.setTheme(isDark() ? "vs-dark" : "vs");
  });

  $effect(() => {
    const key = editorState.active;
    untrack(() => show(key));
  });

  function rememberView() {
    if (!editor || !shownKey) return;
    session.views[shownKey] = editor.saveViewState();
    saveSession();
  }

  function fileOf(key: string): OpenFile | undefined {
    return editorState.files.find((f) => f.key === key);
  }

  async function show(key: string) {
    if (!monaco || !editor) return;
    if (shownKey && docs.has(shownKey)) docs.get(shownKey)!.view = editor.saveViewState();
    const file = fileOf(key);
    if (!file) {
      editor.setModel(null);
      shownKey = "";
      return;
    }
    let doc = docs.get(key);
    if (!doc) {
      try {
        const [content, path] = await Promise.all([api.fileRead(file.siteId, file.rel), api.localPath(file.siteId, file.rel)]);
        const uri = monaco.Uri.file(path);
        const model = monaco.editor.getModel(uri) ?? monaco.editor.createModel(content, undefined, uri);
        const view = (session.views[key] as Monaco.editor.ICodeEditorViewState | undefined) ?? null;
        doc = { model, saved: model.getAlternativeVersionId(), view };
        docs.set(key, doc);
        const d = doc;
        model.onDidChangeContent(() => {
          const f = fileOf(key);
          if (f) f.dirty = model.getAlternativeVersionId() !== d.saved;
        });
      } catch (e) {
        log(`${file.rel}: ${te(e)}`, "error");
        removeTab(key);
        return;
      }
    }
    if (editorState.active !== key) return;
    shownKey = "";
    editor.setModel(doc.model);
    if (doc.view) editor.restoreViewState(doc.view);
    editor.focus();
    shownKey = key;
  }

  async function save(upload: boolean) {
    const file = fileOf(editorState.active);
    const doc = docs.get(editorState.active);
    if (!file || !doc) return;
    try {
      await api.fileWrite(file.siteId, file.rel, doc.model.getValue());
      doc.saved = doc.model.getAlternativeVersionId();
      file.dirty = false;
      const site = ui.sites.find((s) => s.id === file.siteId);
      if (upload || site?.uploadOnSave) {
        log(t("Gespeichert: {path}, wird hochgeladen …", { path: file.rel }));
        await api.upload(file.siteId, [file.rel]);
      } else {
        log(t("Gespeichert: {path}", { path: file.rel }), "ok");
      }
    } catch (e) {
      log(`${t("Speichern")} ${file.rel}: ${te(e)}`, "error");
    }
  }

  function removeTab(key: string) {
    const idx = editorState.files.findIndex((f) => f.key === key);
    if (idx < 0) return;
    const siteId = editorState.files[idx].siteId;
    editorState.files.splice(idx, 1);
    docs.get(key)?.model.dispose();
    docs.delete(key);
    delete session.views[key];
    saveSession();
    if (shownKey === key) shownKey = "";
    if (editorState.active !== key) return;
    const sameSite = editorState.files.filter((f) => f.siteId === siteId);
    if (sameSite.length === 0) {
      editorState.active = "";
      ui.currentId = siteId;
      ui.tab = "files";
      return;
    }
    const next = editorState.files[Math.min(idx, editorState.files.length - 1)];
    editorState.active = next.siteId === siteId ? next.key : sameSite[sameSite.length - 1].key;
  }

  async function close(f: OpenFile) {
    if (f.dirty && !(await confirm(t("{name} hat ungespeicherte Änderungen. Trotzdem schliessen?", { name: f.name }), { kind: "warning" }))) return;
    removeTab(f.key);
  }

  function onKey(e: KeyboardEvent) {
    if (ui.tab !== "editor" || !e.ctrlKey || e.key.toLowerCase() !== "w") return;
    e.preventDefault();
    e.stopPropagation();
    const f = fileOf(editorState.active);
    if (f) close(f);
  }

  const active = $derived(editorState.files.find((f) => f.key === editorState.active));
  const activeSite = $derived(active ? ui.sites.find((s) => s.id === active.siteId) : undefined);
</script>

<svelte:window onkeydowncapture={onKey} />

<div class="editor-view">
  <div class="editor-tabs">
    {#each editorState.files as f (f.key)}
      <div class="etab" class:active={f.key === editorState.active} title={f.rel}>
        <button class="etab-name" onclick={() => (editorState.active = f.key)}>{f.name}{f.dirty ? " ●" : ""}</button>
        <button class="etab-close" title={t("Schliessen (Ctrl+W)")} onclick={() => close(f)}>×</button>
      </div>
    {/each}
    <div class="spacer"></div>
    <button onclick={() => save(false)} disabled={!active} title="Ctrl+S">{t("Speichern")}</button>
    <button onclick={() => save(true)} disabled={!active} title="Ctrl+Shift+U">{t("Speichern & hochladen")}</button>
  </div>
  <div class="editor-host" bind:this={host}></div>
  {#if !active}
    <div class="editor-empty">{t("Datei per Doppelklick in der Dateiansicht öffnen.")}</div>
  {/if}
  <div class="editor-status">
    <span>{active ? `${activeSite?.name ?? ""} / ${active.rel}` : ""}</span>
    <span>{activeSite?.uploadOnSave ? t("Hochladen beim Speichern: an") : ""}</span>
    <span>{active ? cursor : ""}</span>
  </div>
</div>
