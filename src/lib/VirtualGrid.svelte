<script lang="ts">
  import { t } from "./i18n.svelte";
  import { src } from "./api";
  import { fmtDay, originLabel } from "./format";
  import type { PhotoList } from "./photoList.svelte";
  import { store } from "./store.svelte";
  import TimelineScrubber from "./TimelineScrubber.svelte";

  let {
    list,
    onopen,
    onselect,
    ondayaction,
    onsection,
    collapsed = [],
    ontoggle,
    showOrigin = false,
  }: {
    list: PhotoList;
    onopen: (index: number) => void;
    onselect: (index: number, e: MouseEvent) => void;
    /** Header buttons: select a day's photos, or make an album from them. */
    ondayaction?: (action: "select" | "album", day: string, start: number, count: number) => void;
    /** Section headings (album sorted by section): rename, reorder or delete a section, or
     * drop photos (`ids`) on it. Section 0 is "No section". */
    onsection?: (action: "rename" | "delete" | "drop", section: number, arg: { name?: string; ids?: number[] }) => void;
    /** Sections whose photos are hidden, and the heading caret that hides or shows them. */
    collapsed?: number[];
    ontoggle?: (section: number) => void;
    /** Badge each photo with where it came from (phone, camera, LINE…). */
    showOrigin?: boolean;
  } = $props();

  const HEADER = 44;
  const GAP = 6;
  const PAD = 16;
  const OVERSCAN = 800;

  let el: HTMLDivElement;
  let width = $state(0);
  let height = $state(0);
  let scrollTop = $state(0);

  /** Right margin; wider for multi-day lists, leaving room for the timeline scrubber. */
  const padRight = $derived(list.buckets.length > 1 ? 44 : PAD);
  const cols = $derived(Math.max(1, Math.floor((width - PAD - padRight + GAP) / (store.thumbSize + GAP))));
  const cell = $derived(Math.max(40, (width - PAD - padRight - GAP * (cols - 1)) / cols));

  type Row =
    | { kind: "header"; top: number; day: string; count: number; start: number; section: number | null; label: string }
    | { kind: "photos"; top: number; start: number; n: number };

  const layout = $derived.by(() => {
    const rows: Row[] = [];
    let top = PAD;
    let index = 0;
    for (const b of list.buckets) {
      if (b.day || b.section != null) {
        rows.push({ kind: "header", top, day: b.day, count: b.count, start: index, section: b.section, label: b.label });
        top += HEADER;
      }
      const hidden = b.section != null && collapsed.includes(b.section);
      for (let i = 0; !hidden && i < b.count; i += cols) {
        rows.push({ kind: "photos", top, start: index + i, n: Math.min(cols, b.count - i) });
        top += cell + GAP;
      }
      index += b.count;
    }
    return { rows, height: top + PAD };
  });

  const visible = $derived.by(() => {
    const rows = layout.rows;
    const lo = scrollTop - OVERSCAN;
    const hi = scrollTop + height + OVERSCAN;
    let a = 0;
    let b = rows.length;
    while (a < b) {
      const m = (a + b) >> 1;
      if (rows[m].top + cell < lo) a = m + 1;
      else b = m;
    }
    const out: Row[] = [];
    for (let i = a; i < rows.length && rows[i].top < hi; i++) out.push(rows[i]);
    return out;
  });

  const cells = $derived(
    visible.flatMap((r) =>
      r.kind === "photos" ? Array.from({ length: r.n }, (_, k) => ({ i: r.start + k, top: r.top, col: k })) : [],
    ),
  );

  const days = $derived(
    layout.rows.flatMap((r) => (r.kind === "header" && r.day ? [{ day: r.day, top: r.top }] : [])),
  );
  /** Day-grouped and long enough that scrolling alone gets tedious. */
  const showScrubber = $derived(days.length > 1 && layout.height > height * 3);

  /** Label of the day (or section) at the top of the viewport. */
  const currentDay = $derived.by(() => {
    let label = "";
    for (const r of layout.rows) {
      if (r.top > scrollTop + 8) break;
      if (r.kind === "header") label = r.section != null ? r.label || t("No section") : fmtDay(r.day);
    }
    return scrollTop > HEADER ? label : "";
  });

  /** Section whose name is being edited, and the one photos are dragged over. */
  let editing = $state<number | null>(null);
  let dropOn = $state<number | null>(null);
  const isPhotoDrag = (e: DragEvent) => !!e.dataTransfer?.types.includes("application/x-foto-ids");

  function commitName(section: number, old: string, name: string) {
    editing = null;
    if (name.trim() && name.trim() !== old) onsection?.("rename", section, { name: name.trim() });
  }

  function dropPhotos(e: DragEvent, section: number) {
    e.preventDefault();
    dropOn = null;
    const raw = e.dataTransfer?.getData("application/x-foto-ids");
    if (raw) onsection?.("drop", section, { ids: JSON.parse(raw) });
  }

  const focusSelect = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };

  $effect(() => {
    if (cells.length) list.ensure(cells[0].i, cells[cells.length - 1].i);
  });

  export function scrollToTop() {
    if (el) el.scrollTop = 0;
    scrollTop = 0;
  }

  export function scrollToIndex(index: number) {
    const row = layout.rows.find((r) => r.kind === "photos" && index >= r.start && index < r.start + r.n);
    if (!row || !el) return;
    if (row.top < el.scrollTop || row.top + cell > el.scrollTop + height) {
      el.scrollTop = row.top - height / 2 + cell / 2;
    }
  }

  function dragStart(e: DragEvent, id: number) {
    const ids = store.selection.has(id) ? [...store.selection] : [id];
    e.dataTransfer?.setData("application/x-foto-ids", JSON.stringify(ids));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
  }
</script>

<div class="wrap">
<div class="grid" id="photo-grid" class:scrubbed={showScrubber} bind:this={el} bind:clientWidth={width} bind:clientHeight={height} onscroll={() => (scrollTop = el.scrollTop)}>
  <div class="spacer" style:height="{layout.height}px">
    {#each visible as row (row.top)}
      {#if row.kind === "header" && row.section != null}
        {@const id = row.section}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <h3 class="day section" class:drop={dropOn === id} style:top="{row.top}px"
          ondragover={(e) => { if (onsection && isPhotoDrag(e)) { e.preventDefault(); dropOn = id; } }}
          ondragleave={() => dropOn === id && (dropOn = null)}
          ondrop={(e) => dropPhotos(e, id)}>
          {#if ontoggle}
            {@const open = !collapsed.includes(id)}
            <button class="link caret" aria-label={open ? t("Collapse") : t("Expand")} aria-expanded={open}
              onclick={() => ontoggle(id)}>{open ? "▾" : "▸"}</button>
          {/if}
          {#if editing === id}
            <input class="secname" value={row.label} use:focusSelect
              onblur={(e) => commitName(id, row.label, e.currentTarget.value)}
              onkeydown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") editing = null;
                e.stopPropagation();
              }} />
          {:else}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="name" class:none={!id} title={id && onsection ? t("Double-click to rename") : ""}
              ondblclick={() => id && onsection && (editing = id)}>{row.label || t("No section")}</span>
          {/if}
          <span class="count">{row.count}</span>
          {#if ondayaction && row.count}
            <button class="link dayact" onclick={() => ondayaction("select", "", row.start, row.count)}>{t("Select")}</button>
          {/if}
          {#if id && onsection}
            <button class="link dayact" title={t("Delete section")} onclick={() => onsection("delete", id, { name: row.label })}>×</button>
          {/if}
        </h3>
      {:else if row.kind === "header"}
        <h3 class="day" style:top="{row.top}px">
          {fmtDay(row.day)} <span class="count">{row.count}</span>
          {#if ondayaction}
            <button class="link dayact" onclick={() => ondayaction("select", row.day, row.start, row.count)}>{t("Select")}</button>
            <button class="link dayact" title={t("Create an album from this day's photos")}
              onclick={() => ondayaction("album", row.day, row.start, row.count)}>＋ {t("New album")}</button>
          {/if}
        </h3>
      {/if}
    {/each}
    {#each cells as c (c.i)}
      {@const p = list.get(c.i)}
      <button
        class="cell"
        class:selected={p && store.selection.has(p.id)}
        style:top="{c.top}px"
        style:left="{PAD + c.col * (cell + GAP)}px"
        style:width="{cell}px"
        style:height="{cell}px"
        draggable={!!p}
        ondragstart={(e) => p && dragStart(e, p.id)}
        onclick={(e) => onselect(c.i, e)}
        ondblclick={() => onopen(c.i)}>
        {#if p}
          <img src={src(p.thumb)} alt={p.origName} loading="lazy" decoding="async" draggable="false" />
          {#if p.mark}<span class="mark mark-{p.mark} markbadge">{p.mark}</span>{/if}
          {#if showOrigin && originLabel(p)}
            <span class="origin" class:custom={!!p.source}>{originLabel(p)}</span>
          {/if}
          {#if p.favorite || p.rating}
            <span class="badges">
              {#if p.favorite}<span class="fav">♥</span>{/if}
              {#if p.rating}<span class="rating">{"★".repeat(p.rating)}</span>{/if}
            </span>
          {/if}
        {/if}
      </button>
    {/each}
  </div>
</div>
{#if currentDay}<div class="pill">{currentDay}</div>{/if}
{#if showScrubber}
  <TimelineScrubber {days} contentHeight={layout.height} viewHeight={height} {scrollTop}
    onscroll={(top) => (el.scrollTop = top)} />
{/if}
</div>

<style>
  .wrap { position: relative; height: 100%; }
  .grid { position: relative; overflow-y: auto; height: 100%; }
  /* The timeline scrubber replaces the scrollbar. */
  .grid.scrubbed { scrollbar-width: none; }
  .grid.scrubbed::-webkit-scrollbar { display: none; }
  .spacer { position: relative; }
  .day {
    position: absolute; left: 16px; right: 16px; height: 44px; margin: 0;
    display: flex; align-items: center; gap: 8px; font-size: 15px; font-weight: 600;
  }
  .day .count { color: var(--muted); font-weight: 400; font-size: 13px; }
  .section.drop { outline: 2px dashed var(--accent); outline-offset: -2px; border-radius: 6px; }
  .section .none { color: var(--muted); font-weight: 400; }
  .secname { font-size: 14px; width: 260px; }
  .caret { width: 18px; margin-left: -4px; font-size: 11px; color: var(--muted); }
  .dayact { font-size: 12px; font-weight: 400; margin-left: 6px; visibility: hidden; }
  .day:hover .dayact { visibility: visible; }
  .cell {
    position: absolute; padding: 0; border: 2px solid transparent; border-radius: 6px;
    background: var(--cell); overflow: hidden; cursor: default;
  }
  .cell img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .cell.selected { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent); }
  .cell.selected img { opacity: 0.85; }
  .badges {
    position: absolute; left: 4px; bottom: 4px; display: flex; gap: 4px; padding: 1px 5px;
    border-radius: 4px; background: rgb(0 0 0 / 0.55); font-size: 11px;
  }
  .fav { color: #ff6b81; }
  .markbadge { position: absolute; top: 4px; right: 4px; box-shadow: 0 1px 4px rgb(0 0 0 / 0.5); }
  .origin {
    position: absolute; top: 4px; left: 4px; padding: 1px 6px; border-radius: 4px; font-size: 11px;
    background: rgb(0 0 0 / 0.55); color: #ddd;
  }
  .origin.custom { background: var(--accent); color: #fff; }
  .rating { color: var(--star); letter-spacing: -1px; }
  .pill {
    position: absolute; top: 8px; left: 16px;
    padding: 4px 10px; border-radius: 999px; background: var(--panel); border: 1px solid var(--border);
    font-size: 12px; box-shadow: 0 2px 8px rgb(0 0 0 / 0.3); pointer-events: none;
  }
</style>
