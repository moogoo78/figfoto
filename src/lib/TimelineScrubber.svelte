<script lang="ts">
  import { i18n } from "./i18n.svelte";

  let {
    days,
    contentHeight,
    viewHeight,
    scrollTop,
    onscroll,
  }: {
    /** Day headers in display order, with their offset in the grid. */
    days: { day: string; top: number }[];
    contentHeight: number;
    viewHeight: number;
    scrollTop: number;
    onscroll: (top: number) => void;
  } = $props();

  const MIN_LABEL_GAP = 18;
  const MIN_TICK_GAP = 5;

  let track: HTMLDivElement;
  let trackHeight = $state(0);
  /** Pointer position on the track while hovering or dragging. */
  let hoverY = $state<number | null>(null);
  let dragging = $state(false);

  const maxScroll = $derived(Math.max(1, contentHeight - viewHeight));
  /** Track position for a grid offset: where the thumb is when that offset is at the top of the view. */
  const yOf = (top: number) => (Math.min(top, maxScroll) / maxScroll) * trackHeight;

  /** First day of each month, and which of those start a year, thinned so labels don't overlap. */
  const marks = $derived.by(() => {
    const out: { y: number; year: string | null }[] = [];
    let month = "";
    let lastLabel = -Infinity;
    let lastTick = -Infinity;
    for (const d of days) {
      const m = d.day.slice(0, 7);
      if (m === month) continue;
      const newYear = m.slice(0, 4) !== month.slice(0, 4);
      month = m;
      const y = yOf(d.top);
      if (newYear && y - lastLabel >= MIN_LABEL_GAP) {
        out.push({ y, year: m.slice(0, 4) });
        lastLabel = lastTick = y;
      } else if (y - lastTick >= MIN_TICK_GAP) {
        out.push({ y, year: null });
        lastTick = y;
      }
    }
    return out;
  });

  /** The day shown at the top of the view if the thumb were at track position `y`. */
  function dayAt(y: number): string {
    const target = (Math.max(0, Math.min(y, trackHeight)) / trackHeight) * maxScroll;
    let [lo, hi] = [0, days.length - 1];
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (days[mid].top <= target + 1) lo = mid;
      else hi = mid - 1;
    }
    return days[lo]?.day ?? "";
  }

  function monthLabel(day: string): string {
    const d = new Date(`${day.slice(0, 7)}-01T00:00:00`);
    return isNaN(d.getTime()) ? day : d.toLocaleDateString(i18n.locale, { year: "numeric", month: "short" });
  }

  function seek(e: PointerEvent) {
    const y = e.clientY - track.getBoundingClientRect().top;
    hoverY = Math.max(0, Math.min(y, trackHeight));
    onscroll((hoverY / trackHeight) * maxScroll);
  }
</script>

<div
  class="scrubber"
  class:active={dragging}
  bind:this={track}
  bind:clientHeight={trackHeight}
  role="scrollbar"
  aria-orientation="vertical"
  aria-controls="photo-grid"
  aria-valuemin={0}
  aria-valuemax={Math.round(maxScroll)}
  aria-valuenow={Math.round(scrollTop)}
  tabindex="-1"
  onpointerdown={(e) => {
    dragging = true;
    track.setPointerCapture(e.pointerId);
    seek(e);
  }}
  onpointermove={(e) => {
    if (dragging) seek(e);
    else hoverY = e.clientY - track.getBoundingClientRect().top;
  }}
  onpointerup={() => (dragging = false)}
  onpointercancel={() => (dragging = false)}
  onpointerleave={() => !dragging && (hoverY = null)}>
  {#each marks as m (m.y)}
    {#if m.year}
      <span class="year" style:top="{m.y}px">{m.year}</span>
    {:else}
      <span class="tick" style:top="{m.y}px"></span>
    {/if}
  {/each}
  <span class="thumb" style:top="{yOf(scrollTop)}px"></span>
  {#if hoverY != null && days.length}
    <span class="hover" style:top="{hoverY}px">{monthLabel(dayAt(hoverY))}</span>
  {/if}
</div>

<style>
  .scrubber {
    position: absolute; top: 8px; bottom: 8px; right: 0; width: 56px;
    cursor: ns-resize; user-select: none; touch-action: none;
  }
  .year, .tick, .thumb, .hover { position: absolute; right: 8px; transform: translateY(-50%); pointer-events: none; }
  .year { font-size: 11px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .tick { width: 4px; height: 4px; border-radius: 50%; background: var(--border); right: 14px; }
  .scrubber:hover .year, .active .year { color: var(--fg); }
  .thumb { right: 0; width: 4px; height: 28px; border-radius: 2px; background: var(--accent); }
  .hover {
    right: 18px; padding: 3px 10px; border-radius: 999px; white-space: nowrap; font-size: 12px;
    background: var(--panel); border: 1px solid var(--border); box-shadow: 0 2px 8px rgb(0 0 0 / 0.3);
  }
  .hover::after {
    content: ""; position: absolute; right: -18px; top: 50%; width: 14px; height: 1px; background: var(--accent);
  }
</style>
