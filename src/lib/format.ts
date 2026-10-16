// Isto formatiranje kao u Rust/Python verziji: 1,2 MB · 12.345 · 1 fajl / 2 fajla / 5 fajlova.

export function human(n: number): string {
  let v = n;
  for (const unit of ["B", "KB", "MB", "GB", "TB"]) {
    if (v < 1024 || unit === "TB") {
      return unit === "B" ? `${n} B` : `${v.toFixed(1).replace(".", ",")} ${unit}`;
    }
    v /= 1024;
  }
  return "";
}

export function num(n: number): string {
  return Math.round(n)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ".");
}

export function pluralWord(n: number, one: string, few: string, many: string): string {
  const a = n % 10;
  const b = n % 100;
  if (a === 1 && b !== 11) return one;
  if (a >= 2 && a <= 4 && !(b >= 12 && b <= 14)) return few;
  return many;
}

export const plural = (n: number, one: string, few: string, many: string) => `${num(n)} ${pluralWord(n, one, few, many)}`;
export const nFiles = (n: number) => plural(n, "fajl", "fajla", "fajlova");
export const nConvs = (n: number) => plural(n, "razgovor", "razgovora", "razgovora");
export const nProjs = (n: number) => plural(n, "projekat", "projekta", "projekata");
export const nDays = (n: number) => plural(n, "dan", "dana", "dana");

export function ago(epochSecs: number): string {
  if (!epochSecs) return "";
  const s = Date.now() / 1000 - epochSecs;
  if (s < 90) return "upravo";
  const m = s / 60;
  if (m < 60) return `pre ${plural(Math.round(m), "minut", "minuta", "minuta")}`;
  const h = m / 60;
  if (h < 24) return `pre ${plural(Math.round(h), "sat", "sata", "sati")}`;
  const d = h / 24;
  if (d < 2) return "juče";
  if (d < 31) return `pre ${nDays(Math.round(d))}`;
  const mo = d / 30.4;
  if (mo < 12) return `pre ${plural(Math.round(mo), "mesec", "meseca", "meseci")}`;
  return `pre ${plural(Math.round(mo / 12), "godinu", "godine", "godina")}`;
}

export function years(days: number): string {
  if (days >= 365 && days % 365 === 0) return plural(days / 365, "godina", "godine", "godina");
  if (days >= 3650) return `~${Math.round(days / 365)} godina`;
  return nDays(days);
}

export const pad2 = (n: number) => n.toString().padStart(2, "0");

export const FREQ_TEXT: Record<string, string> = {
  daily: "svakog dana",
  weekly: "svakog ponedeljka",
  monthly: "1. u mesecu",
};

/** ISO vreme iz MANIFEST-a → 16.10.2026. 23:26 (lokalno). Ostalo vraća kako jeste. */
export function fmtWhen(s: string): string {
  if (!/^\d{4}-\d{2}-\d{2}T/.test(s)) return s;
  const d = new Date(s.replace(/(\.\d{3})\d+/, "$1"));
  if (isNaN(+d)) return s;
  return `${pad2(d.getDate())}.${pad2(d.getMonth() + 1)}.${d.getFullYear()}. ${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}
