<script lang="ts">
  import { t } from "./i18n.svelte";
  import { store } from "./store.svelte";
  import SaveInput from "./SaveInput.svelte";

  /** Sets the lens by hand, e.g. for a manual lens that records none. Empty clears it. */
  let { onset, disabled = false }: { onset: (lens: string) => void; disabled?: boolean } = $props();
</script>

<SaveInput
  {onset}
  {disabled}
  suggestions={store.lenses.map((l) => l.name)}
  placeholder={t("Lens (manual lenses)")}
  title={disabled
    ? t("The camera recorded the lens for this photo.")
    : t("Set the lens for photos taken with a manual lens. Enter nothing to go back to what the camera recorded.")} />
