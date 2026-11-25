import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ClipItem, FilterKind, Settings } from "./types";

export const api = {
  list: (query: string, kind: FilterKind, limit = 300) =>
    invoke<ClipItem[]>("list_items", { query, kind, limit }),
  get: (id: number) => invoke<ClipItem | null>("get_item", { id }),
  paste: (id: number, plain = false) => invoke<void>("paste_item", { id, plain }),
  copy: (id: number, plain = false) => invoke<void>("copy_item", { id, plain }),
  remove: (id: number) => invoke<void>("delete_item", { id }),
  clearAll: (keepPinned: boolean) => invoke<number>("clear_all", { keepPinned }),
  setFlag: (id: number, field: "pinned" | "favorite", value: boolean) =>
    invoke<void>("set_flag", { id, field, value }),
  setNote: (id: number, note: string) => invoke<void>("set_note", { id, note }),
  updateText: (id: number, text: string) => invoke<void>("update_text", { id, text }),
  stats: () => invoke<[number, number]>("stats"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  hidePanel: () => invoke<void>("hide_panel"),
  openSettings: () => invoke<void>("open_settings"),
  closeSettings: () => invoke<void>("close_settings"),
  pruneNow: () => invoke<number>("prune_now"),
  activatePanel: () => invoke<void>("activate_panel"),
  setPanelPinned: (pinned: boolean) => invoke<void>("set_panel_pinned", { pinned }),
  getPanelPinned: () => invoke<boolean>("get_panel_pinned"),
  quit: () => invoke<void>("quit_app"),
};

export function fileUrl(path: string | null | undefined): string {
  if (!path) return "";
  try {
    return convertFileSrc(path);
  } catch {
    return "";
  }
}
