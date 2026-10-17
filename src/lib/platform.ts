// Mac vs Windows wording and paths. In a plain browser you can try ?os=windows.
import { L } from "./i18n";

const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
const forced = typeof location !== "undefined" ? new URLSearchParams(location.search).get("os") : null;

export const OS: "windows" | "macos" | "linux" =
  forced === "windows" || forced === "macos" ? forced : /Windows/i.test(ua) ? "windows" : /Mac/i.test(ua) ? "macos" : "linux";
export const isWin = OS === "windows";
export const isMac = OS === "macos";
export const SEP = isWin ? "\\" : "/";

export const T = {
  /** "Show in Finder" / "Prikaži u Finderu" */
  get inFm() {
    return isWin ? L("Exploreru", "Explorer") : L("Finderu", "Finder");
  },
  get fm() {
    return isWin ? "Explorer" : "Finder";
  },
  /** the system that starts scheduled backups */
  get os() {
    return isWin ? "Windows" : "macOS";
  },
  get computer() {
    return isWin ? L("računar", "computer") : "Mac";
  },
  get Computer() {
    return isWin ? L("Računar", "Computer") : "Mac";
  },
  get onThis() {
    return isWin ? L("na ovom računaru", "on this computer") : L("na ovom Mac-u", "on this Mac");
  },
  get withYour() {
    return isWin ? L("sa tvojim računarom", "with your computer") : L("sa tvojim Mac-om", "with your Mac");
  },
  get newComputer() {
    return isWin ? L("novi računar", "a new computer") : L("novi Mac", "a new Mac");
  },
};
