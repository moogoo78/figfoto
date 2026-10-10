import { zhTW } from "./locales/zh-TW";

export type Locale = "en" | "zh-TW";

export const LOCALES: { value: Locale; label: string }[] = [
  { value: "en", label: "EN" },
  { value: "zh-TW", label: "中文" },
];

const TABLES: Record<Locale, Record<string, string>> = { en: {}, "zh-TW": zhTW };
const STORAGE_KEY = "frutyfoto.locale";

function detect(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "en" || saved === "zh-TW") return saved;
  } catch {}
  const lang = navigator.language.toLowerCase();
  return lang.startsWith("zh") && !/^zh-(cn|sg|hans)/.test(lang) ? "zh-TW" : "en";
}

class I18n {
  locale = $state<Locale>(detect());

  constructor() {
    document.documentElement.lang = this.locale;
  }

  set(locale: Locale) {
    this.locale = locale;
    document.documentElement.lang = locale;
    try {
      localStorage.setItem(STORAGE_KEY, locale);
    } catch {}
  }
}

export const i18n = new I18n();

/**
 * Translates an English UI string. The English text is the key, so a missing
 * translation falls back to English. `{name}` placeholders are filled from `vars`.
 */
export function t(text: string, vars?: Record<string, string | number>): string {
  const s = TABLES[i18n.locale][text] ?? text;
  return vars ? s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : s;
}
