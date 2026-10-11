<script lang="ts">
  import { t } from "./i18n.svelte";

  /** Free-text field for one photo. Saves on blur (or Ctrl+Enter) when the text changed. */
  let {
    id,
    value,
    placeholder,
    onsave,
  }: { id: number; value: string | null; placeholder: string; onsave: (text: string) => void } = $props();

  let text = $state("");
  // Reset when moving to another photo or when the saved value changes.
  $effect(() => {
    void id;
    text = value ?? "";
  });

  function save() {
    if (text.trim() !== (value ?? "")) onsave(text.trim());
  }
</script>

<textarea
  bind:value={text}
  {placeholder}
  rows="3"
  title={t("Saves when you click elsewhere. Ctrl+Enter also saves.")}
  onblur={save}
  onkeydown={(e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) e.currentTarget.blur();
    else if (e.key === "Escape") {
      text = value ?? "";
      e.currentTarget.blur();
    }
    e.stopPropagation();
  }}></textarea>

<style>
  textarea { display: block; width: 100%; min-height: 72px; resize: vertical; font-size: 13px; }
</style>
