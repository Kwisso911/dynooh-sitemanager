<script lang="ts" module>
  export interface MenuItem {
    label?: string;
    hint?: string;
    action?: () => void;
    disabled?: boolean;
    danger?: boolean;
    separator?: boolean;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";

  let { x, y, items, onclose }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();

  let el = $state<HTMLDivElement | null>(null);
  let pos = $state({ left: 0, top: 0 });

  onMount(() => {
    const r = el!.getBoundingClientRect();
    pos = {
      left: Math.min(x, window.innerWidth - r.width - 4),
      top: Math.min(y, window.innerHeight - r.height - 4),
    };
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== "Escape") return;
      if (e.type === "pointerdown" && el?.contains(e.target as Node)) return;
      onclose();
    };
    window.addEventListener("pointerdown", close, true);
    window.addEventListener("keydown", close, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("keydown", close, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
    };
  });

  function run(item: MenuItem) {
    if (item.disabled) return;
    onclose();
    item.action?.();
  }
</script>

<div class="ctx" bind:this={el} style="left: {pos.left || x}px; top: {pos.top || y}px" role="menu" tabindex="-1" oncontextmenu={(e) => e.preventDefault()}>
  {#each items as item}
    {#if item.separator}
      <div class="ctx-sep"></div>
    {:else}
      <button class="ctx-item" class:danger={item.danger} disabled={item.disabled} role="menuitem" onclick={() => run(item)}>
        <span>{item.label}</span>
        {#if item.hint}<span class="ctx-hint">{item.hint}</span>{/if}
      </button>
    {/if}
  {/each}
</div>
