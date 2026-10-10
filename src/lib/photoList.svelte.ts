import { api } from "./api";
import type { Bucket, Filter, Photo, Sort } from "./types";

const PAGE = 200;
/** Pages kept in memory; the ones farthest from the visible range are dropped first. */
const MAX_PAGES = 12;

/**
 * A lazily paged view of the photos matching a filter. Day buckets give the full
 * layout up front; photo rows are fetched page by page as they scroll into view.
 */
export class PhotoList {
  buckets = $state.raw<Bucket[]>([]);
  total = $state(0);
  /** False until the first reload finishes. */
  loaded = $state(false);
  pages = $state.raw<Record<number, Photo[]>>({});
  /** Loaded photos by id. */
  byId = $derived.by(() => {
    const map = new Map<number, Photo>();
    for (const page of Object.values(this.pages)) for (const p of page) map.set(p.id, p);
    return map;
  });

  #filter: Filter = { tagIds: [], trashed: false };
  #sort: Sort = "takenDesc";
  #gen = 0;
  #loading = new Map<number, Promise<void>>();
  /** Page range last requested by the grid. */
  #visible: [number, number] = [0, 0];

  /** Switches to a new filter/sort, dropping all cached pages. */
  async reload(filter: Filter, sort: Sort) {
    const gen = ++this.#gen;
    this.#filter = filter;
    this.#sort = sort;
    this.#loading.clear();
    const buckets = await api.dateBuckets(filter, sort);
    if (gen !== this.#gen) return;
    this.#setBuckets(buckets);
    this.pages = {};
    this.loaded = true;
  }

  /** Re-fetches the current filter while keeping loaded pages on screen until replaced. */
  async refresh() {
    const gen = ++this.#gen;
    this.#loading.clear();
    const loaded = Object.keys(this.pages).map(Number);
    const [buckets, ...pages] = await Promise.all([
      api.dateBuckets(this.#filter, this.#sort),
      ...loaded.map((p) => api.listPhotos(this.#filter, this.#sort, p * PAGE, PAGE)),
    ]);
    if (gen !== this.#gen) return;
    this.#setBuckets(buckets as Bucket[]);
    const next: Record<number, Photo[]> = {};
    loaded.forEach((p, i) => (next[p] = pages[i] as Photo[]));
    this.pages = next;
  }

  get(i: number): Photo | undefined {
    return this.pages[Math.floor(i / PAGE)]?.[i % PAGE];
  }

  /** A loaded photo by id (photos far from the visible range may not be loaded). */
  find(id: number): Photo | undefined {
    return this.byId.get(id);
  }

  /** Makes sure indexes `from..=to` are loaded (or loading). */
  ensure(from: number, to: number) {
    const [a, b] = [Math.floor(Math.max(0, from) / PAGE), Math.floor(Math.max(0, to) / PAGE)];
    this.#visible = [a, b];
    for (let p = a; p <= b; p++) this.#load(p);
  }

  async fetch(i: number): Promise<Photo | undefined> {
    if (i < 0 || i >= this.total) return undefined;
    await this.#load(Math.floor(i / PAGE));
    return this.get(i);
  }

  /** All ids matching the current filter (for select-all). */
  allIds(): Promise<number[]> {
    return api.listPhotoIds(this.#filter, this.#sort, 0, this.total);
  }

  /** Ids at indexes `start..start+count` (e.g. one day's photos). */
  idsInRange(start: number, count: number): Promise<number[]> {
    return api.listPhotoIds(this.#filter, this.#sort, start, count);
  }

  #setBuckets(buckets: Bucket[]) {
    this.buckets = buckets;
    this.total = buckets.reduce((n, b) => n + b.count, 0);
  }

  #load(page: number): Promise<void> {
    if (this.pages[page]) return Promise.resolve();
    const pending = this.#loading.get(page);
    if (pending) return pending;
    const gen = this.#gen;
    const promise = api.listPhotos(this.#filter, this.#sort, page * PAGE, PAGE).then(
      (photos) => {
        if (gen !== this.#gen) return;
        this.#loading.delete(page);
        this.pages = this.#evict({ ...this.pages, [page]: photos }, page);
      },
      () => {
        this.#loading.delete(page);
      },
    );
    this.#loading.set(page, promise);
    return promise;
  }

  /** Drops the pages farthest from the visible range (never it, nor `keep`) beyond `MAX_PAGES`. */
  #evict(pages: Record<number, Photo[]>, keep: number): Record<number, Photo[]> {
    const loaded = Object.keys(pages).map(Number);
    if (loaded.length <= MAX_PAGES) return pages;
    const [a, b] = this.#visible;
    const distance = (p: number) => (p === keep || (p >= a && p <= b) ? -1 : p < a ? a - p : p - b);
    const drop = loaded.sort((x, y) => distance(y) - distance(x)).slice(0, loaded.length - MAX_PAGES);
    const next = { ...pages };
    for (const p of drop) if (distance(p) >= 0) delete next[p];
    return next;
  }
}
