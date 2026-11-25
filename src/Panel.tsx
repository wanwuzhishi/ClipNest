import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, fileUrl } from "./api";
import type { ClipItem, FilterKind } from "./types";
import {
  IconEmpty,
  IconFiles,
  IconImage,
  IconPin,
  IconSearch,
  IconSettings,
  IconStar,
  IconText,
} from "./ui";

function timeAgo(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 60_000) return "刚刚";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  const d = new Date(ms);
  const now = new Date();
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  if (d.toDateString() === now.toDateString()) return `今天 ${hh}:${mm}`;
  const y = new Date(now.getTime() - 86_400_000);
  if (d.toDateString() === y.toDateString()) return `昨天 ${hh}:${mm}`;
  return `${d.getMonth() + 1}-${d.getDate()} ${hh}:${mm}`;
}

function humanSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function kindLabel(kind: string): string {
  if (kind === "image") return "图片";
  if (kind === "files") return "文件";
  return "文本";
}

const FILTERS: { key: FilterKind; label: string }[] = [
  { key: "all", label: "全部" },
  { key: "text", label: "文本" },
  { key: "image", label: "图片" },
  { key: "files", label: "文件" },
  { key: "favorite", label: "收藏" },
];

export default function Panel() {
  const [items, setItems] = useState<ClipItem[]>([]);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<FilterKind>("all");
  const [sel, setSel] = useState(0);
  const [paused, setPaused] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const toastTimer = useRef<number | null>(null);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 1300);
  }, []);

  const refresh = useCallback(
    async (q: string, f: FilterKind) => {
      try {
        const list = await api.list(q, f, 300);
        setItems(list);
        setSel((s) => Math.min(s, Math.max(0, list.length - 1)));
      } catch (e) {
        console.error(e);
      }
    },
    []
  );

  // 搜索防抖
  useEffect(() => {
    const t = window.setTimeout(() => refresh(query, filter), 60);
    return () => window.clearTimeout(t);
  }, [query, filter, refresh]);

  // 后端事件
  useEffect(() => {
    const un: Promise<() => void>[] = [
      listen<ClipItem>("clip-added", () => refresh(query, filter)),
      listen("history-cleared", () => refresh(query, filter)),
      listen<boolean>("paused-changed", (e) => setPaused(e.payload)),
      listen("panel-shown", () => {
        setQuery("");
        setFilter("all");
        setSel(0);
        inputRef.current?.focus();
        refresh("", "all");
      }),
    ];
    api.getSettings().then((s) => setPaused(s.paused)).catch(() => {});
    inputRef.current?.focus();
    return () => {
      un.forEach((p) => p.then((f) => f()).catch(() => {}));
    };
  }, [query, filter, refresh]);

  // 选中项滚动到可视区
  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-idx="${sel}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [sel, items]);

  const doPaste = useCallback(
    async (item: ClipItem, plain: boolean) => {
      try {
        await api.paste(item.id, plain);
      } catch (e) {
        showToast(`粘贴失败：${e}`);
      }
    },
    [showToast]
  );

  const doCopy = useCallback(
    async (item: ClipItem, plain: boolean) => {
      try {
        await api.copy(item.id, plain);
        showToast(plain ? "已复制为纯文本" : "已复制到剪贴板");
      } catch (e) {
        showToast(`复制失败：${e}`);
      }
    },
    [showToast]
  );

  const doDelete = useCallback(
    async (item: ClipItem) => {
      await api.remove(item.id);
      await refresh(query, filter);
      showToast("已删除");
    },
    [query, filter, refresh, showToast]
  );

  const toggleFlag = useCallback(
    async (item: ClipItem, field: "pinned" | "favorite") => {
      const next = !(field === "pinned" ? item.pinned : item.favorite);
      await api.setFlag(item.id, field, next);
      await refresh(query, filter);
      showToast(field === "pinned" ? (next ? "已置顶" : "已取消置顶") : next ? "已收藏" : "已取消收藏");
    },
    [query, filter, refresh, showToast]
  );

  // 全局键盘操作
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const inInput =
        !!target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      const current = items[sel];

      if (e.key === "Escape") {
        e.preventDefault();
        api.hidePanel();
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSel((s) => Math.min(s + 1, Math.max(0, items.length - 1)));
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSel((s) => Math.max(s - 1, 0));
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        if (current) doPaste(current, e.shiftKey);
        return;
      }
      if (!inInput && e.key === "Delete") {
        e.preventDefault();
        if (current) doDelete(current);
        return;
      }
      if (!inInput && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "p") {
        e.preventDefault();
        if (current) toggleFlag(current, "pinned");
        return;
      }
      if (!inInput && !e.ctrlKey && !e.altKey && !query && /^[1-9]$/.test(e.key)) {
        const it = items[Number(e.key) - 1];
        if (it) {
          e.preventDefault();
          doPaste(it, false);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [items, sel, query, doPaste, doDelete, toggleFlag]);

  const current = items[sel];
  const counts = useMemo(() => {
    let text = 0;
    let image = 0;
    let files = 0;
    let fav = 0;
    for (const it of items) {
      if (it.kind === "image") image++;
      else if (it.kind === "files") files++;
      else text++;
      if (it.favorite) fav++;
    }
    return { all: items.length, text, image, files, favorite: fav };
  }, [items]);

  return (
    <div className="shell">
      <div className="panel">
        <div className="topbar drag-region">
          <div className="search no-drag">
            <IconSearch />
            <input
              ref={inputRef}
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setSel(0);
              }}
              placeholder="搜索剪贴板历史…"
              spellCheck={false}
              autoComplete="off"
            />
          </div>
          <div className="topbar-meta no-drag">
            {paused && <span style={{ color: "#ffc46b" }}>已暂停</span>}
            <button className="icon-btn" title="设置" onClick={() => api.openSettings()}>
              <IconSettings />
            </button>
          </div>
        </div>

        <div className="tabs no-drag">
          {FILTERS.map((f) => (
            <button
              key={f.key}
              className={`tab ${filter === f.key ? "active" : ""}`}
              onClick={() => {
                setFilter(f.key);
                setSel(0);
              }}
            >
              {f.label}
              <span className="count">{counts[f.key]}</span>
            </button>
          ))}
        </div>

        <div className="body">
          <div className="list" ref={listRef}>
            {items.length === 0 && (
              <div className="empty">
                <div className="empty-icon">
                  <IconEmpty />
                </div>
                <div>{query ? "没有匹配的记录" : "还没有剪贴板记录"}</div>
                <small>{query ? "换个关键词试试" : "复制点什么，这里就会出现"}</small>
              </div>
            )}
            {items.map((it, i) => (
              <div
                key={it.id}
                data-idx={i}
                className={`item ${i === sel ? "selected" : ""}`}
                onMouseMove={() => setSel(i)}
                onDoubleClick={() => doPaste(it, false)}
              >
                <div className="item-icon">
                  {it.kind === "image" && it.imagePath ? (
                    <img src={fileUrl(it.imagePath)} alt="" draggable={false} />
                  ) : it.kind === "files" ? (
                    <IconFiles />
                  ) : it.kind === "image" ? (
                    <IconImage />
                  ) : (
                    <IconText />
                  )}
                </div>
                <div className="item-main">
                  <div className={`item-text ${it.kind === "text" ? "" : "mono"}`}>
                    {it.preview || kindLabel(it.kind)}
                  </div>
                  <div className="item-meta">
                    <span>{timeAgo(it.updatedAt)}</span>
                    <span className="dot" />
                    <span>{kindLabel(it.kind)}</span>
                    {it.kind !== "files" && (
                      <>
                        <span className="dot" />
                        <span>{humanSize(it.size)}</span>
                      </>
                    )}
                    {it.copies > 1 && (
                      <>
                        <span className="dot" />
                        <span>{it.copies} 次</span>
                      </>
                    )}
                  </div>
                </div>
                <div className="item-badges">
                  {it.pinned && (
                    <span className="badge pin" title="已置顶">
                      <IconPin />
                    </span>
                  )}
                  {it.favorite && (
                    <span className="badge fav" title="已收藏">
                      <IconStar />
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>

          <div className="preview">
            {current ? (
              <>
                <div className="preview-head">
                  <div className="preview-title">
                    {kindLabel(current.kind)} · {humanSize(current.size)} · {current.copies} 次复制
                  </div>
                  <div className="preview-actions">
                    <button className="mini-btn" onClick={() => toggleFlag(current, "pinned")}>
                      {current.pinned ? "取消置顶" : "置顶"}
                    </button>
                    <button className="mini-btn" onClick={() => toggleFlag(current, "favorite")}>
                      {current.favorite ? "取消收藏" : "收藏"}
                    </button>
                    <button className="mini-btn" onClick={() => doCopy(current, true)}>
                      纯文本
                    </button>
                    <button className="mini-btn primary" onClick={() => doPaste(current, false)}>
                      粘贴
                    </button>
                  </div>
                </div>
                <div className={`preview-body ${current.kind === "text" ? "" : ""}`}>
                  {current.kind === "image" && current.imagePath ? (
                    <img src={fileUrl(current.imagePath)} alt="预览" />
                  ) : current.kind === "files" && current.files ? (
                    <div className="file-list">
                      {current.files.map((p) => (
                        <div className="file-row" key={p}>
                          <IconFiles />
                          <span className="path" title={p}>
                            {p}
                          </span>
                        </div>
                      ))}
                    </div>
                  ) : (
                    (current.text ?? current.preview).slice(0, 6000)
                  )}
                </div>
              </>
            ) : (
              <div className="empty">
                <div className="empty-icon">
                  <IconEmpty />
                </div>
                <div>选中一条记录查看详情</div>
              </div>
            )}
          </div>
        </div>

        <div className="footer">
          <div className="hints">
            <span className="hint">
              <kbd>↑</kbd>
              <kbd>↓</kbd> 选择
            </span>
            <span className="hint">
              <kbd>Enter</kbd> 粘贴
            </span>
            <span className="hint">
              <kbd>Shift</kbd>+<kbd>Enter</kbd> 纯文本
            </span>
            <span className="hint">
              <kbd>Ctrl</kbd>+<kbd>P</kbd> 置顶
            </span>
            <span className="hint">
              <kbd>Del</kbd> 删除
            </span>
          </div>
          <div className="hints">
            <span className="hint">
              <kbd>Esc</kbd> 关闭
            </span>
          </div>
        </div>
      </div>
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
