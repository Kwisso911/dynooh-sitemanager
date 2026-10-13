<script lang="ts">
  import { closeSettings, settings, ui } from "$lib/state.svelte";
  import { clamp, loadLayout, saveLayout } from "$lib/drag";
  import { t } from "$lib/i18n.svelte";
  import SiteForm from "./SiteForm.svelte";

  const MIN_W = 520;
  const MIN_H = 260;
  const EDGES = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];

  let cardEl = $state<HTMLDivElement | null>(null);
  let pos = $state<{ x: number; y: number } | null>(null);
  let size = $state(loadLayout<{ w: number; h: number | null }>("settingsSize", { w: 860, h: null }));
  let vw = $state(0);
  let vh = $state(0);

  const tabs = $derived(settings.open.map((id) => ui.sites.find((s) => s.id === id)).filter((s) => !!s));

  $effect(() => {
    if (settings.open.length === 0) pos = null;
  });

  $effect(() => {
    if (cardEl && !pos) {
      const r = cardEl.getBoundingClientRect();
      pos = { x: Math.max(10, Math.round((innerWidth - r.width) / 2)), y: Math.max(10, Math.round((innerHeight - r.height) / 2)) };
    }
  });

  $effect(() => {
    for (const id of settings.open) if (ui.sites.length && !ui.sites.some((s) => s.id === id)) closeSettings(id);
  });

  const style = $derived.by(() => {
    const width = `width: ${Math.min(size.w, vw - 20)}px`;
    const height = size.h ? `height: ${size.h}px` : "height: auto";
    if (!pos) return `${width}; ${height}; max-height: calc(100vh - 20px); visibility: hidden; left: 0; top: 0`;
    return `${width}; ${height}; max-height: ${vh - pos.y - 10}px; left: ${pos.x}px; top: ${pos.y}px`;
  });

  function track(e: PointerEvent, onMove: (dx: number, dy: number) => void, onEnd?: () => void) {
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    const cx = e.clientX;
    const cy = e.clientY;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => onMove(ev.clientX - cx, ev.clientY - cy);
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      onEnd?.();
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  function startMove(e: PointerEvent) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button") || !pos || !cardEl) return;
    const start = { ...pos };
    const w = cardEl.offsetWidth;
    track(e, (dx, dy) => {
      pos = { x: clamp(start.x + dx, 80 - w, innerWidth - 80), y: clamp(start.y + dy, 0, innerHeight - 40) };
    });
  }

  function startResize(e: PointerEvent, edge: string) {
    if (e.button !== 0 || !pos || !cardEl) return;
    const r = cardEl.getBoundingClientRect();
    track(
      e,
      (dx, dy) => {
        let { left: x, top: y, width: w, height: h } = r;
        if (edge.includes("e")) w = clamp(r.width + dx, MIN_W, innerWidth - r.left - 10);
        if (edge.includes("w")) {
          w = clamp(r.width - dx, MIN_W, r.right - 10);
          x = r.right - w;
        }
        if (edge.includes("s")) h = clamp(r.height + dy, MIN_H, innerHeight - r.top - 10);
        if (edge.includes("n")) {
          h = clamp(r.height - dy, MIN_H, r.bottom);
          y = r.bottom - h;
        }
        pos = { x, y };
        size = { w, h: edge === "e" || edge === "w" ? size.h : h };
      },
      () => saveLayout("settingsSize", size),
    );
  }

  let tabsEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const id = settings.active;
    void tabs.length;
    tabsEl?.querySelector(`[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  });

  $effect(() => {
    const el = tabsEl;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      if (el.scrollWidth <= el.clientWidth) return;
      e.preventDefault();
      el.scrollLeft += e.deltaY || e.deltaX;
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });

  function resetSize() {
    size = { w: 860, h: null };
    saveLayout("settingsSize", size);
  }
</script>

<svelte:window bind:innerWidth={vw} bind:innerHeight={vh} />

{#if tabs.length}
  <div class="settings-card" bind:this={cardEl} {style} role="dialog" aria-label={t("Site-Einstellungen")}>
    {#each EDGES as edge}
      <div
        class="rz rz-{edge}"
        role="separator"
        title={t("Grösse ändern (Doppelklick: an Inhalt anpassen)")}
        onpointerdown={(e) => startResize(e, edge)}
        ondblclick={resetSize}
      ></div>
    {/each}
    <div class="settings-head" role="toolbar" tabindex="-1" onpointerdown={startMove}>
      <span class="settings-title">{t("Einstellungen")}</span>
      <div class="stabs" bind:this={tabsEl}>
        {#each tabs as s (s.id)}
          <div class="stab" class:active={s.id === settings.active} data-id={s.id}>
            <button
              class="stab-label"
              onclick={() => (settings.active = s.id)}
              onauxclick={(e) => e.button === 1 && closeSettings(s.id)}
              title={s.name}
            >
              {s.name}
            </button>
            <button class="stab-close" title={t("Tab schliessen")} onclick={() => closeSettings(s.id)}>×</button>
          </div>
        {/each}
      </div>
      <button class="icon settings-close" title={t("Schliessen")} onclick={() => closeSettings()}>×</button>
    </div>
    <div class="settings-body">
      {#each tabs as s (s.id)}
        <div class="view-pane" class:hidden={s.id !== settings.active}>
          <SiteForm site={s} />
        </div>
      {/each}
    </div>
  </div>
{/if}
