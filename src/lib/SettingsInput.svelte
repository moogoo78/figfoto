<script lang="ts">
  import { t } from "./i18n.svelte";
  import { store } from "./store.svelte";
  import SaveInput from "./SaveInput.svelte";

  /** Sets the camera-settings note on many photos at once. Empty clears it. */
  let { onset }: { onset: (text: string) => void } = $props();
</script>

<SaveInput
  {onset}
  width={200}
  suggestions={store.settingsNotes}
  placeholder={t("Camera settings (tripod, bulb…)")}
  title={t("Set the camera-settings note on the selected photos. It replaces what they had; enter nothing to clear it.")} />
