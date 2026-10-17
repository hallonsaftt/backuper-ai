// Razlike u tekstu i putanjama između Mac-a i Windowsa. U pregledaču se može probati sa ?os=windows.
const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
const forced = typeof location !== "undefined" ? new URLSearchParams(location.search).get("os") : null;

export const OS: "windows" | "macos" | "linux" =
  forced === "windows" || forced === "macos" ? forced : /Windows/i.test(ua) ? "windows" : /Mac/i.test(ua) ? "macos" : "linux";
export const isWin = OS === "windows";
export const isMac = OS === "macos";
export const SEP = isWin ? "\\" : "/";

export const T = {
  /** „Prikaži u Finderu / Exploreru” */
  inFm: isWin ? "Exploreru" : "Finderu",
  fm: isWin ? "Explorer" : "Finder",
  /** sistem koji pokreće zakazani backup */
  os: isWin ? "Windows" : "macOS",
  computer: isWin ? "računar" : "Mac",
  Computer: isWin ? "Računar" : "Mac",
  /** „na ovom Mac-u / računaru” */
  onThis: isWin ? "na ovom računaru" : "na ovom Mac-u",
  /** „sa tvojim Mac-om / računarom” */
  withYour: isWin ? "sa tvojim računarom" : "sa tvojim Mac-om",
  newComputer: isWin ? "novi računar" : "novi Mac",
};
