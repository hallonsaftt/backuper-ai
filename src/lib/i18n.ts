// Two languages: English (default) and Serbian. The choice lives in the backend (prefs.json),
// so scheduled backups and the command line use it too; localStorage only speeds up the first paint.
export type Lang = "en" | "sr";

function initial(): Lang {
  try {
    const q = new URLSearchParams(location.search).get("lang");
    if (q === "sr" || q === "en") return q;
    const v = localStorage.getItem("cb-lang");
    if (v === "sr" || v === "en") return v;
  } catch {
    /* no storage */
  }
  return "en";
}

let current: Lang = initial();

export const getLang = () => current;
export const isSr = () => current === "sr";

export function setLangLocal(l: Lang) {
  current = l;
  try {
    localStorage.setItem("cb-lang", l);
  } catch {
    /* no storage */
  }
  document.documentElement.lang = l;
}

/** L("Srpski tekst", "English text") */
export function L(sr: string, en: string): string {
  return current === "sr" ? sr : en;
}
