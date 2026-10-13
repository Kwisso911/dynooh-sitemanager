<script lang="ts">
  import { untrack } from "svelte";
  import { api, type GitStatus, type Site } from "$lib/api";
  import { log, openFile } from "$lib/state.svelte";
  import { t, te } from "$lib/i18n.svelte";

  let { site }: { site: Site } = $props();

  let status = $state<GitStatus | null>(null);
  let message = $state("");
  let busy = $state(false);
  let output = $state("");

  async function refresh() {
    try {
      status = await api.gitStatus(site.id);
    } catch (e) {
      log(`Git: ${te(e)}`, "error");
    }
  }

  $effect(() => {
    void site.id;
    untrack(() => {
      status = null;
      output = "";
      refresh();
    });
  });

  async function run(label: string, fn: () => Promise<string>) {
    busy = true;
    try {
      output = await fn();
      log(`${label} OK`, "ok");
      if (label === "Commit") message = "";
    } catch (e) {
      output = te(e);
      log(`${label}: ${te(e)}`, "error");
    } finally {
      busy = false;
      await refresh();
    }
  }

  const CODE: Record<string, string> = { M: "geändert", A: "neu", D: "gelöscht", R: "umbenannt", "?": "neu", U: "Konflikt" };
  function describe(code: string) {
    const c = code.trim()[0] ?? "";
    return CODE[c] ? t(CODE[c]) : code;
  }

  function relToSite(path: string): string | null {
    if (!status) return null;
    const root = status.root.replace(/\\/g, "/").toLowerCase();
    const local = site.localPath.replace(/\\/g, "/").replace(/\/$/, "").toLowerCase();
    const prefix = local.startsWith(root) ? local.slice(root.length).replace(/^\//, "") : "";
    if (!prefix) return path;
    return path.toLowerCase().startsWith(prefix + "/") ? path.slice(prefix.length + 1) : null;
  }
</script>

<div class="git-view">
  {#if !status}
    <div class="hint">{t("Lade …")}</div>
  {:else if !status.isRepo}
    <div class="hint">
      <p>{t("{path} ist kein Git-Repository.", { path: site.localPath })}</p>
      <button onclick={() => run("Init", () => api.gitInit(site.id))} disabled={busy}>{t("Repository initialisieren")}</button>
    </div>
  {:else}
    <div class="toolbar">
      <span class="badge">{status.branch || "–"}</span>
      {#if status.upstream}<span class="muted">→ {status.upstream}</span>{/if}
      {#if status.ahead}<span class="badge ok">{status.ahead} ↑</span>{/if}
      {#if status.behind}<span class="badge warn">{status.behind} ↓</span>{/if}
      <span class="muted">{status.remoteUrl || t("kein Remote «origin»")}</span>
      <div class="spacer"></div>
      <button onclick={refresh} disabled={busy}>{t("Aktualisieren")}</button>
      <button onclick={() => run("Pull", () => api.gitPull(site.id))} disabled={busy || !status.upstream}>Pull</button>
      <button onclick={() => run("Push", () => api.gitPush(site.id))} disabled={busy || !status.remoteUrl}>Push</button>
    </div>
    <div class="git-body">
      <div class="git-files">
        <h3>{t("Änderungen")} ({status.files.length})</h3>
        {#if status.files.length === 0}
          <div class="hint">{t("Keine lokalen Änderungen.")}</div>
        {:else}
          <table class="files">
            <tbody>
              {#each status.files as f (f.path)}
                {@const rel = relToSite(f.path)}
                <tr ondblclick={() => rel && openFile(site.id, rel)}>
                  <td class="code">{f.code}</td>
                  <td>{f.path}</td>
                  <td class="muted">{describe(f.code)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
      <div class="git-commit">
        <textarea bind:value={message} placeholder={t("Commit-Nachricht")} rows="4"></textarea>
        <div class="row">
          <button class="primary" onclick={() => run("Commit", () => api.gitCommit(site.id, message))} disabled={busy || !message.trim() || status.files.length === 0}>
            {t("Alle Änderungen committen")}
          </button>
          <button
            onclick={async () => {
              await run("Commit", () => api.gitCommit(site.id, message));
              await run("Push", () => api.gitPush(site.id));
            }}
            disabled={busy || !message.trim() || status.files.length === 0 || !status.remoteUrl}
          >
            Commit &amp; Push
          </button>
        </div>
        {#if output}<pre class="output">{output}</pre>{/if}
      </div>
    </div>
  {/if}
</div>
