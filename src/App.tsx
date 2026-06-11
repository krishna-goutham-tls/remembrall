import { Routes, Route } from "react-router-dom";
import MenubarPanel from "./components/MenubarPanel";
import BrowseView from "./components/BrowseView";
import SettingsView from "./components/SettingsView";
import FTUEWizard from "./components/FTUEWizard";

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<MenubarPanel />} />
      <Route path="/browse" element={<BrowseView />} />
      <Route path="/settings" element={<SettingsView />} />
      <Route path="/ftue" element={<FTUEWizard />} />
    </Routes>
  );
}
