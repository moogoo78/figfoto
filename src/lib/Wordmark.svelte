<script lang="ts">
  import "@fontsource/bricolage-grotesque/800.css";
  import mark from "../assets/mark.svg";

  /** Font size in px; the mark matches the cap height and the gap scales with it. */
  let { size = 20 }: { size?: number } = $props();
</script>

<span class="wordmark" style:font-size="{size}px" style:gap="{(size * 12) / 44}px">
  <img src={mark} alt="" /><span><span class="fruty">fruty</span><span class="foto">foto</span></span>
</span>

<style>
  .wordmark { display: inline-flex; align-items: center; font-family: "Bricolage Grotesque", system-ui, sans-serif;
    font-weight: 800; letter-spacing: -0.04em; line-height: 1; }
  img { height: 1em; width: auto; }
  .fruty { color: var(--brand-ink); }
  .foto { color: var(--brand-amber); }
</style>
