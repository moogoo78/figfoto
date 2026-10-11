/** Briefly flashes an element green to show something was saved. */
export function flash(el: Element | null | undefined) {
  if (!el) return;
  el.classList.remove("flash");
  void (el as HTMLElement).offsetWidth; // restart the animation
  el.classList.add("flash");
}
