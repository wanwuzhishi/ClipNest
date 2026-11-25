import { useMemo } from "react";
import Panel from "./Panel";
import SettingsPage from "./SettingsPage";

export default function App() {
  const view = useMemo(
    () => new URLSearchParams(window.location.search).get("view"),
    []
  );
  return view === "settings" ? <SettingsPage /> : <Panel />;
}
