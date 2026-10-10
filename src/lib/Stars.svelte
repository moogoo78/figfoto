<script lang="ts">
  import { t } from "./i18n.svelte";
  let { value, onchange }: { value: number; onchange: (n: number) => void } = $props();
  let hover = $state(0);
</script>

<span class="stars" role="group" aria-label={t("Rating")} onmouseleave={() => (hover = 0)}>
  {#each [1, 2, 3, 4, 5] as n}
    <button
      type="button"
      class:on={n <= (hover || value)}
      title={n === value ? t("Clear rating") : t(n > 1 ? "{n} stars" : "{n} star", { n })}
      onmouseenter={() => (hover = n)}
      onclick={() => onchange(n === value ? 0 : n)}>★</button>
  {/each}
</span>

<style>
  .stars { display: inline-flex; }
  button {
    background: none; border: none; padding: 0 1px; font-size: 18px; line-height: 1;
    color: var(--muted); cursor: pointer;
  }
  button.on { color: var(--star); }
</style>
