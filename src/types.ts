export type ClipKind = "text" | "image" | "files";

export interface ClipItem {
  id: number;
  kind: ClipKind;
  preview: string;
  text: string | null;
  html: string | null;
  rtf: string | null;
  imagePath: string | null;
  thumbPath: string | null;
  files: string[] | null;
  size: number;
  sourceApp: string | null;
  pinned: boolean;
  favorite: boolean;
  note: string | null;
  copies: number;
  createdAt: number;
  updatedAt: number;
}

export interface Settings {
  hotkey: string;
  keepCount: number;
  keepDays: number;
  maxTextKb: number;
  pasteHide: boolean;
  clickPaste: boolean;
  privacyFilter: boolean;
  excludedApps: string;
  skipSensitive: boolean;
  startWithWindows: boolean;
  theme: string;
  paused: boolean;
}

export type FilterKind = "all" | "text" | "image" | "files" | "favorite";
