<script lang="ts">
  import { untrack } from "svelte";
  import { api } from "$lib/api";
  import { log } from "$lib/state.svelte";
  import { t, te } from "$lib/i18n.svelte";
  import { prefs } from "$lib/prefs.svelte";
  import { finishMaster, master, type MasterMode } from "$lib/master.svelte";

  let { mode: initial }: { mode: MasterMode } = $props();

  let step = $state<MasterMode | "reset">(untrack(() => initial));
  let password = $state("");
  let repeat = $state("");
  let email = $state("");
  let code = $state("");
  let sentTo = $state("");
  let error = $state("");
  let busy = $state(false);
  let firstInput = $state<HTMLInputElement | null>(null);

  $effect(() => {
    void step;
    firstInput?.focus();
  });

  const mandatory = $derived(step === "setup");
  const mismatch = $derived(step !== "unlock" && repeat.length > 0 && repeat !== password);

  async function run(fn: () => Promise<void>) {
    busy = true;
    error = "";
    try {
      await fn();
    } catch (e) {
      error = te(e);
    } finally {
      busy = false;
    }
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy) return;
    if (step === "unlock") {
      run(async () => {
        await api.masterUnlock(password);
        finishMaster(true);
      });
    } else if (step === "setup") {
      if (password.length < 8) return void (error = t("Mindestens 8 Zeichen"));
      if (password !== repeat) return void (error = t("Die Passwörter stimmen nicht überein"));
      run(async () => {
        await api.masterSetup(password, email);
        log(t("Master-Passwort eingerichtet"), "ok");
        finishMaster(true);
      });
    } else {
      if (password.length < 8) return void (error = t("Mindestens 8 Zeichen"));
      if (password !== repeat) return void (error = t("Die Passwörter stimmen nicht überein"));
      run(async () => {
        await api.masterReset(code, password);
        log(t("Master-Passwort zurückgesetzt"), "ok");
        finishMaster(true);
      });
    }
  }

  function forgot() {
    run(async () => {
      sentTo = await api.masterForgot(prefs.lang);
      password = "";
      repeat = "";
      code = "";
      step = "reset";
    });
  }

  function cancel() {
    if (!mandatory && !busy) finishMaster(false);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && cancel()} />

<div class="modal-backdrop master-backdrop" role="presentation">
  <form class="modal master-modal" onsubmit={submit}>
    <div class="modal-head">
      <strong>
        {step === "setup" ? t("Master-Passwort einrichten") : step === "unlock" ? t("Master-Passwort eingeben") : t("Master-Passwort zurücksetzen")}
      </strong>
      <div class="spacer"></div>
      {#if !mandatory}
        <button type="button" class="icon" title={t("Schliessen")} onclick={cancel} disabled={busy}>×</button>
      {/if}
    </div>

    {#if step === "setup"}
      <p class="muted">
        {t("Das Master-Passwort schützt das Anzeigen, Kopieren und Exportieren gespeicherter Passwörter. Die E-Mail-Adresse wird für «Passwort vergessen» verwendet.")}
      </p>
      <label>{t("Master-Passwort")} <input bind:this={firstInput} type="password" autocomplete="new-password" bind:value={password} /></label>
      <label>{t("Passwort wiederholen")} <input type="password" autocomplete="new-password" bind:value={repeat} /></label>
      <label>{t("E-Mail-Adresse")} <input type="email" autocomplete="email" bind:value={email} placeholder="name@example.ch" /></label>
    {:else if step === "unlock"}
      <p class="muted">{t("Zum Anzeigen von Passwörtern bitte das Master-Passwort eingeben. Die Freigabe gilt 10 Minuten.")}</p>
      <label>{t("Master-Passwort")} <input bind:this={firstInput} type="password" autocomplete="current-password" bind:value={password} /></label>
      <div class="master-forgot">
        <button type="button" class="link" onclick={forgot} disabled={busy} title={master.email ? t("Code an {email} senden", { email: master.email }) : undefined}>
          {t("Passwort vergessen?")}
        </button>
      </div>
    {:else}
      <p class="muted">{t("Ein Code wurde an {email} gesendet. Er ist 30 Minuten gültig.", { email: sentTo })}</p>
      <label>{t("Code aus der E-Mail")} <input bind:this={firstInput} inputmode="numeric" autocomplete="one-time-code" maxlength="6" bind:value={code} /></label>
      <div class="master-forgot">
        <button type="button" class="link" onclick={forgot} disabled={busy}>{t("Code erneut senden")}</button>
      </div>
      <label>{t("Neues Master-Passwort")} <input type="password" autocomplete="new-password" bind:value={password} /></label>
      <label>{t("Passwort wiederholen")} <input type="password" autocomplete="new-password" bind:value={repeat} /></label>
    {/if}

    {#if error}
      <p class="error-text">{error}</p>
    {:else if mismatch}
      <p class="error-text">{t("Die Passwörter stimmen nicht überein")}</p>
    {/if}

    <div class="row master-actions">
      <div class="spacer"></div>
      {#if !mandatory}
        <button type="button" onclick={cancel} disabled={busy}>{t("Abbrechen")}</button>
      {/if}
      <button class="primary" type="submit" disabled={busy}>
        {busy ? t("Bitte warten …") : step === "unlock" ? t("Entsperren") : t("Speichern")}
      </button>
    </div>
  </form>
</div>
