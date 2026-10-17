// Same formatting as the Rust core: English 1.2 MB · 12,345 · 1 file / 2 files;
// srpski 1,2 MB · 12.345 · 1 fajl / 2 fajla / 5 fajlova.
import { isSr, L } from "./i18n";

export function human(n: number): string {
  let v = n;
  for (const unit of ["B", "KB", "MB", "GB", "TB"]) {
    if (v < 1024 || unit === "TB") {
      if (unit === "B") return `${n} B`;
      const s = v.toFixed(1);
      return `${isSr() ? s.replace(".", ",") : s} ${unit}`;
    }
    v /= 1024;
  }
  return "";
}

export function num(n: number): string {
  return Math.round(n)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, isSr() ? "." : ",");
}

export function pluralWord(n: number, one: string, few: string, many: string): string {
  const a = n % 10;
  const b = n % 100;
  if (a === 1 && b !== 11) return one;
  if (a >= 2 && a <= 4 && !(b >= 12 && b <= 14)) return few;
  return many;
}

/** Number + word in the current language: pl(5, ["fajl", "fajla", "fajlova"], ["file", "files"]) */
export function plWord(n: number, sr: [string, string, string], en: [string, string]): string {
  return isSr() ? pluralWord(n, sr[0], sr[1], sr[2]) : n === 1 ? en[0] : en[1];
}
export const pl = (n: number, sr: [string, string, string], en: [string, string]) => `${num(n)} ${plWord(n, sr, en)}`;

export const nFiles = (n: number) => pl(n, ["fajl", "fajla", "fajlova"], ["file", "files"]);
export const nConvs = (n: number) => pl(n, ["razgovor", "razgovora", "razgovora"], ["conversation", "conversations"]);
export const nProjs = (n: number) => pl(n, ["projekat", "projekta", "projekata"], ["project", "projects"]);
export const nDays = (n: number) => pl(n, ["dan", "dana", "dana"], ["day", "days"]);

export function ago(epochSecs: number): string {
  if (!epochSecs) return "";
  const s = Date.now() / 1000 - epochSecs;
  if (s < 90) return L("upravo", "just now");
  const m = s / 60;
  if (m < 60) return isSr() ? `pre ${pl(Math.round(m), ["minut", "minuta", "minuta"], ["", ""])}` : `${pl(Math.round(m), ["", "", ""], ["minute", "minutes"])} ago`;
  const h = m / 60;
  if (h < 24) return isSr() ? `pre ${pl(Math.round(h), ["sat", "sata", "sati"], ["", ""])}` : `${pl(Math.round(h), ["", "", ""], ["hour", "hours"])} ago`;
  const d = h / 24;
  if (d < 2) return L("juče", "yesterday");
  if (d < 31) return isSr() ? `pre ${nDays(Math.round(d))}` : `${nDays(Math.round(d))} ago`;
  const mo = d / 30.4;
  if (mo < 12) return isSr() ? `pre ${pl(Math.round(mo), ["mesec", "meseca", "meseci"], ["", ""])}` : `${pl(Math.round(mo), ["", "", ""], ["month", "months"])} ago`;
  const y = Math.round(mo / 12);
  return isSr() ? `pre ${pl(y, ["godinu", "godine", "godina"], ["", ""])}` : `${pl(y, ["", "", ""], ["year", "years"])} ago`;
}

/** "ago" without the word "ago" — for tight columns */
export function agoShort(epochSecs: number): string {
  return ago(epochSecs).replace(/^pre /, "").replace(/ ago$/, "");
}

export function years(days: number): string {
  const yr = (n: number) => pl(n, ["godina", "godine", "godina"], ["year", "years"]);
  if (days >= 365 && days % 365 === 0) return yr(days / 365);
  if (days >= 3650) return `~${yr(Math.round(days / 365))}`;
  return nDays(days);
}

export const pad2 = (n: number) => n.toString().padStart(2, "0");

export function freqText(freq: string): string {
  if (freq === "daily") return L("svakog dana", "every day");
  if (freq === "weekly") return L("svakog ponedeljka", "every Monday");
  return L("1. u mesecu", "on the 1st of the month");
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** ISO time from MANIFEST → "Oct 16, 2026 23:26" / "16.10.2026. 23:26" (local). Anything else is returned as is. */
export function fmtWhen(s: string): string {
  if (!/^\d{4}-\d{2}-\d{2}T/.test(s)) return s;
  const d = new Date(s.replace(/(\.\d{3})\d+/, "$1"));
  if (isNaN(+d)) return s;
  const time = `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  return isSr()
    ? `${pad2(d.getDate())}.${pad2(d.getMonth() + 1)}.${d.getFullYear()}. ${time}`
    : `${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()} ${time}`;
}

/** Today's date, long form, for the overview header */
export function todayLong(): string {
  return new Date().toLocaleDateString(isSr() ? "sr-Latn-RS" : "en-US", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
}
