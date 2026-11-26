import { useEffect, useState } from "react";
import { api } from "./api";
import type { Settings } from "./types";

const DEFAULT: Settings = {
  hotkey: "Alt+V",
  keepCount: 2000,
  keepDays: 30,
  maxTextKb: 4096,
  pasteHide: true,
  clickPaste: false,
  privacyFilter: true,
  excludedApps: "",
  skipSensitive: true,
  startWithWindows: false,
  theme: "dark",
  paused: false,
};

function humanSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export default function SettingsPage() {
  const [s, setS] = useState<Settings>(DEFAULT);
  const [count, setCount] = useState(0);
  const [bytes, setBytes] = useState(0);
  const [msg, setMsg] = useState("");
  const [loaded, setLoaded] = useState(false);

  const loadStats = () => {
    api
      .stats()
      .then(([c, b]) => {
        setCount(c);
        setBytes(b);
      })
      .catch(() => {});
  };

  useEffect(() => {
    api
      .getSettings()
      .then((v) => {
        setS({ ...DEFAULT, ...v });
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
    loadStats();
  }, []);

  const patch = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setS((prev) => ({ ...prev, [key]: value }));

  const save = async () => {
    try {
      await api.saveSettings(s);
      setMsg("已保存");
      window.setTimeout(() => setMsg(""), 1500);
    } catch (e) {
      setMsg(`保存失败：${e}`);
    }
  };

  return (
    <div className="settings-root">
      <div className="settings-head">
        <h1>ClipNest 设置</h1>
        <p>轻量全功能剪贴板 · 数据全部保存在本地</p>
      </div>

      <div className="settings-body">
        <div className="section-title">快捷键与启动</div>

        <div className="field">
          <div className="field-col">
            <label>唤出面板的热键</label>
            <span className="desc">示例：Alt+V、Ctrl+Shift+V、Alt+`</span>
          </div>
          <input
            type="text"
            value={s.hotkey}
            onChange={(e) => patch("hotkey", e.target.value)}
            placeholder="Alt+V"
            disabled={!loaded}
          />
        </div>

        <div className="field">
          <div className="field-col">
            <label>开机自动启动</label>
            <span className="desc">登录 Windows 后静默驻留托盘</span>
          </div>
          <div
            className={`switch ${s.startWithWindows ? "on" : ""}`}
            onClick={() => patch("startWithWindows", !s.startWithWindows)}
          />
        </div>

        <div className="divider" />
        <div className="section-title">记录行为</div>

        <div className="field">
          <div className="field-col">
            <label>最多保留条数</label>
            <span className="desc">置顶与收藏不受限制；0 表示不限制</span>
          </div>
          <input
            type="number"
            value={s.keepCount}
            min={0}
            onChange={(e) => patch("keepCount", Number(e.target.value) || 0)}
          />
        </div>

        <div className="field">
          <div className="field-col">
            <label>最多保留天数</label>
            <span className="desc">0 表示不按时间清理</span>
          </div>
          <input
            type="number"
            value={s.keepDays}
            min={0}
            onChange={(e) => patch("keepDays", Number(e.target.value) || 0)}
          />
        </div>

        <div className="field">
          <div className="field-col">
            <label>粘贴后隐藏面板</label>
            <span className="desc">关闭后粘贴完仍停留在面板上</span>
          </div>
          <div
            className={`switch ${s.pasteHide ? "on" : ""}`}
            onClick={() => patch("pasteHide", !s.pasteHide)}
          />
        </div>

        <div className="field">
          <div className="field-col">
            <label>单击条目直接粘贴</label>
            <span className="desc">关闭时单击只选中，按回车才粘贴</span>
          </div>
          <div
            className={`switch ${s.clickPaste ? "on" : ""}`}
            onClick={() => patch("clickPaste", !s.clickPaste)}
          />
        </div>

        <div className="field">
          <div className="field-col">
            <label>暂停记录</label>
            <span className="desc">临时不抓取新的复制内容</span>
          </div>
          <div
            className={`switch ${s.paused ? "on" : ""}`}
            onClick={() => patch("paused", !s.paused)}
          />
        </div>

        <div className="divider" />
        <div className="section-title">隐私</div>

        <div className="field">
          <div className="field-col">
            <label>启用隐私排除</label>
            <span className="desc">关闭后所有内容都会记录，包括密码和密钥</span>
          </div>
          <div
            className={`switch ${s.privacyFilter ? "on" : ""}`}
            onClick={() => patch("privacyFilter", !s.privacyFilter)}
          />
        </div>

        <div className={`field ${s.privacyFilter ? "" : "disabled"}`}>
          <div className="field-col">
            <label>跳过疑似密钥/卡号</label>
            <span className="desc">私钥、AWS/GitHub/Slack 令牌、JWT、信用卡号不写入历史</span>
          </div>
          <div
            className={`switch ${s.skipSensitive ? "on" : ""}`}
            onClick={() => patch("skipSensitive", !s.skipSensitive)}
          />
        </div>

        <div className={`field-col ${s.privacyFilter ? "" : "disabled"}`} style={{ gap: 6 }}>
          <label>不记录这些程序的复制</label>
          <span className="desc">每行一个 exe 名，不区分大小写；密码管理器默认已列出</span>
          <textarea
            className="settings-textarea"
            rows={6}
            spellCheck={false}
            value={s.excludedApps}
            onChange={(e) => patch("excludedApps", e.target.value)}
            placeholder="KeePass.exe"
          />
        </div>

        <div className="divider" />
        <div className="section-title">数据</div>

        <div className="field">
          <div className="field-col">
            <label>本地占用</label>
            <span className="desc">
              {count} 条记录 · {humanSize(bytes)}
            </span>
          </div>
          <button
            className="btn danger"
            onClick={async () => {
              const n = await api.clearAll(true);
              loadStats();
              setMsg(`已清空 ${n} 条（保留置顶与收藏）`);
              window.setTimeout(() => setMsg(""), 1800);
            }}
          >
            清空历史
          </button>
        </div>

        <div className="field">
          <div className="field-col">
            <label>立即整理数据库</label>
            <span className="desc">按上面的条数与天数规则清理旧记录</span>
          </div>
          <button
            className="btn"
            onClick={async () => {
              const n = await api.pruneNow();
              loadStats();
              setMsg(`清理了 ${n} 条旧记录`);
              window.setTimeout(() => setMsg(""), 1800);
            }}
          >
            立即清理
          </button>
        </div>
      </div>

      <div className="settings-foot">
        <button className="btn" onClick={() => api.closeSettings()}>
          关闭
        </button>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          {msg && <span style={{ fontSize: 11.5, color: "#8fb0ff" }}>{msg}</span>}
          <button className="btn primary" onClick={save}>
            保存设置
          </button>
        </div>
      </div>
    </div>
  );
}
