<script lang="ts">
  import { store } from "./store.svelte";

  /** `<option>`s for an album `<select>`: every album, indented by how deep it is. */
  let { stringIds = false }: { stringIds?: boolean } = $props();
</script>

{#each store.albums as a (a.id)}
  <option value={stringIds ? String(a.id) : a.id}>{"  ".repeat(a.depth)}{a.name}</option>
{/each}
