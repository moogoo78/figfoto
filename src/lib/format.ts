import { i18n, t } from "./i18n.svelte";

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}

/** "2021-07-15" -> "Thu, 15 Jul 2021" */
export function fmtDay(day: string): string {
  const d = new Date(`${day}T00:00:00`);
  if (isNaN(d.getTime())) return day;
  return d.toLocaleDateString(i18n.locale, { weekday: "short", day: "numeric", month: "short", year: "numeric" });
}

/** "240503_" for 2024-05-03: the start of the name of that day's album (240503_Kyoto). */
export function dayAlbumPrefix(day: string): string {
  return /^\d{4}-\d{2}-\d{2}/.test(day) ? `${day.slice(2, 4)}${day.slice(5, 7)}${day.slice(8, 10)}_` : "";
}

export function fmtDateTime(s: string): string {
  const d = new Date(s);
  if (isNaN(d.getTime())) return s;
  return d.toLocaleString(i18n.locale, { dateStyle: "medium", timeStyle: "short" });
}

export function basename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/** Source badge text: the user-set source, else Phone / Camera (empty if unknown). */
export function originLabel(p: { source: string | null; device: string }): string {
  if (p.source) return p.source;
  return p.device === "phone" ? t("Phone") : p.device === "camera" ? t("Camera") : "";
}

export const SOURCE_SUGGESTIONS = ["LINE", "Facebook", "Instagram", "WhatsApp", "Messenger", "Telegram", "WeChat", "Web", "Scan"];

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
