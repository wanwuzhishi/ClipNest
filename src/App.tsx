import { useEffect, useMemo } from "react";
import Panel from "./Panel";
import SettingsPage from "./SettingsPage";

export default function App() {
  // 关掉 WebView 的默认右键菜单（刷新/另存为/检查元素那套）
  useEffect(() => {
    const block = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", block);
    return () => document.removeEventListener("contextmenu", block);
  }, []);

  const view = useMemo(
    () => new URLSearchParams(window.location.search).get("view"),
    []
  );
  return view === "settings" ? <SettingsPage /> : <Panel />;
}
