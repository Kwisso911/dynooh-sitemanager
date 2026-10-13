<script lang="ts">
  import { api } from "$lib/api";
  import { dismissJob, isFinished, jobLabel, jobs } from "$lib/jobs.svelte";
  import { fmtSize, log, ui } from "$lib/state.svelte";
  import { t, te } from "$lib/i18n.svelte";

  const STATE: Record<string, string> = {
    scan: "liest …",
    running: "läuft",
    done: "fertig",
    error: "mit Fehlern",
    cancelled: "abgebrochen",
  };

  const running = $derived(jobs.list.filter((j) => !isFinished(j)));
  const siteName = (id: string) => ui.sites.find((s) => s.id === id)?.name ?? "";

  function cancel(id: number) {
    api.cancelJob(id).catch((e) => log(`${t("Abbrechen")}: ${te(e)}`, "error"));
  }
</script>

{#if jobs.list.length}
  <div class="transfers">
    <div class="transfers-head">
      <strong>{t("Transfers")}</strong>
      <span class="muted">{t("{n} aktiv", { n: running.length })}</span>
      <div class="spacer"></div>
      {#if running.length > 1}
        <button class="danger" onclick={() => api.cancelAllJobs()}>{t("Alle abbrechen")}</button>
      {/if}
    </div>
    {#each jobs.list as j (j.id)}
      {@const pct = j.total ? ((j.done + j.failed) / j.total) * 100 : 0}
      {@const lines = [...(j.scanning && j.scanDir ? [t("Lese {dir}", { dir: j.scanDir })] : []), ...j.current]}
      {@const label = jobLabel(j.label)}
      <div class="job {j.state}">
        <div class="job-label" title={label}>
          <span class="muted">{siteName(j.siteId)}</span>
          {label}
        </div>
        <div class="job-bar">
          <div class="bar" class:indeterminate={j.state === "scan"} style="width: {j.state === 'scan' ? 100 : pct}%"></div>
        </div>
        <div class="job-stats">
          {#if j.state === "scan"}
            {t(STATE.scan)}
          {:else}
            {j.done}/{j.total}{j.scanning ? "+" : ""}{j.failed ? ` · ${t("{n} Fehler", { n: j.failed })}` : ""} · {fmtSize(j.bytes)}
            {#if j.state === "running" && j.rate > 0}· {fmtSize(Math.round(j.rate))}/s{/if}
            {#if isFinished(j)}· {t(STATE[j.state])}{/if}
          {/if}
        </div>
        <div class="job-current muted" title={lines.join("\n")}>{lines.join(" · ")}</div>
        {#if isFinished(j)}
          <button class="icon" title={t("Ausblenden")} onclick={() => dismissJob(j.id)}>×</button>
        {:else}
          <button class="danger" onclick={() => cancel(j.id)}>{t("Abbrechen")}</button>
        {/if}
      </div>
    {/each}
  </div>
{/if}
