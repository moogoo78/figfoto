<script lang="ts">
  import { t } from "./i18n.svelte";
  import { store } from "./store.svelte";

  /** Sets the camera-settings note on many photos at once. Empty clears it. */
  let { onset }: { onset: (text: string) => void } = $props();
  let value = $state("");
  const listId = `settings-${Math.random().toString(36).slice(2)}`;
</script>

<input
  bind:value
  list={listId}
  placeholder={t("Camera settings (tripod, bulb…)")}
  title={t("Set the camera-settings note on the selected photos. It replaces what they had; enter nothing to clear it.")}
  onkeydown={(e) => {
    if (e.key === "Enter") {
      onset(value.trim());
      value = "";
    }
    if (e.key === "Escape") e.currentTarget.blur();
    e.stopPropagation();
  }} />
<datalist id={listId}>
  {#each store.settingsNotes as s}<option value={s}></option>{/each}
</datalist>

<style>
  input { width: 200px; }
</style>
