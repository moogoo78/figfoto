<script lang="ts">
  import { flash } from "./flash";
  import { t } from "./i18n.svelte";

  /** Sets when the selected photos were taken: a date, and optionally a time. Without a time
   * each photo keeps its own; with one, the earliest gets it and the rest keep their spacing. */
  let { onset }: { onset: (date: string, time: string | null) => void } = $props();

  let date = $state("");
  let time = $state("");
  let input: HTMLInputElement | undefined = $state();

  function save() {
    if (!date) return;
    onset(date, time || null);
    flash(input);
    date = "";
    time = "";
  }
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Enter") save();
    if (e.key === "Escape") (e.currentTarget as HTMLInputElement).blur();
    e.stopPropagation();
  };
</script>

<span class="field" title={t("Set when the photos were taken. Without a time each keeps its own; with one, the earliest gets it and the rest keep their spacing.")}>
  <input bind:this={input} type="date" bind:value={date} onkeydown={keys} aria-label={t("Date taken")} />
  <input type="time" bind:value={time} onkeydown={keys} aria-label={t("Time (optional)")} />
  <button disabled={!date} onclick={save}>{t("Set date")}</button>
</span>

<style>
  .field { display: inline-flex; gap: 4px; align-items: center; }
  /* Explicit widths: some WebKitGTK builds otherwise squeeze a text field in a flex row to a sliver. */
  input[type="date"] { width: 140px; }
  input[type="time"] { width: 100px; }
</style>
