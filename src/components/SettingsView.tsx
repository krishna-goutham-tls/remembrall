import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";

type Settings = Record<string, string>;

export default function SettingsView() {
  const [, setSettings] = useState<Settings>({});
  const [droidEnabled, setDroidEnabled] = useState(true);
  const [modelSize, setModelSize] = useState("4b");
  const [clearInput, setClearInput] = useState("");
  const [clearMsg, setClearMsg] = useState("");
  const [redactionLog, setRedactionLog] = useState<string | null>(null);
  const [indexingPaused, setIndexingPaused] = useState(false);
  const [exportMsg, setExportMsg] = useState("");

  useEffect(() => {
    invoke<Settings>("get_settings").then((s) => {
      setSettings(s);
      if (s["droid_enabled"] !== undefined) setDroidEnabled(s["droid_enabled"] === "true");
      if (s["model_size"]) setModelSize(s["model_size"]);
      if (s["indexing_paused"]) setIndexingPaused(s["indexing_paused"] === "true");
    }).catch(console.error);
  }, []);

  const updateSetting = useCallback(async (key: string, value: string) => {
    await invoke("update_setting", { key, value });
    setSettings((prev) => ({ ...prev, [key]: value }));
  }, []);

  const handleOpenDataFolder = useCallback(async () => {
    const home = await invoke<string>("get_settings"); // just to confirm app is alive
    void home; // suppress unused warning
    await open("~/Library/Application Support/Remembrall");
  }, []);

  const handleExport = useCallback(async () => {
    try {
      const json = await invoke<string>("export_memories_json");
      // Create a downloadable blob
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "remembrall-memories.json";
      a.click();
      URL.revokeObjectURL(url);
      setExportMsg("Exported successfully");
      setTimeout(() => setExportMsg(""), 3000);
    } catch (e) {
      setExportMsg("Export failed: " + String(e));
    }
  }, []);

  const handleClearAll = useCallback(async () => {
    if (clearInput !== "DELETE") {
      setClearMsg('Type DELETE to confirm');
      return;
    }
    try {
      await invoke("clear_all_memories");
      setClearMsg("All memories cleared");
      setClearInput("");
    } catch (e) {
      setClearMsg("Failed: " + String(e));
    }
  }, [clearInput]);

  const handleViewRedactionLog = useCallback(async () => {
    try {
      const log = await invoke<string>("get_redaction_log");
      setRedactionLog(log || "(empty)");
    } catch (e) {
      setRedactionLog("Failed to read log: " + String(e));
    }
  }, []);

  const handleToggleIndexing = useCallback(async () => {
    try {
      if (indexingPaused) {
        await invoke("resume_backfill");
        await updateSetting("indexing_paused", "false");
        setIndexingPaused(false);
      } else {
        await invoke("pause_backfill");
        await updateSetting("indexing_paused", "true");
        setIndexingPaused(true);
      }
    } catch (e) {
      console.error("Toggle indexing failed:", e);
    }
  }, [indexingPaused, updateSetting]);

  return (
    <div className="h-screen overflow-y-auto bg-white p-5 text-sm">
      <h1 className="text-lg font-semibold text-gray-800 mb-4">Settings</h1>

      {/* Tools */}
      <section className="mb-5">
        <h2 className="font-medium text-gray-600 mb-2">Tools</h2>
        <div className="space-y-2">
          <label className="flex items-center justify-between">
            <span className="text-gray-700">Droid</span>
            <button
              onClick={() => {
                const next = !droidEnabled;
                setDroidEnabled(next);
                updateSetting("droid_enabled", String(next));
              }}
              className={`relative w-10 h-5 rounded-full transition-colors ${droidEnabled ? "bg-green-500" : "bg-gray-300"}`}
            >
              <span
                className={`absolute top-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${droidEnabled ? "left-5" : "left-0.5"}`}
              />
            </button>
          </label>
          {["Codex", "Claude Code", "Cursor"].map((tool) => (
            <label key={tool} className="flex items-center justify-between opacity-50">
              <span className="text-gray-500">{tool}</span>
              <span className="text-xs text-gray-400">Coming soon</span>
            </label>
          ))}
        </div>
      </section>

      {/* Model */}
      <section className="mb-5">
        <h2 className="font-medium text-gray-600 mb-2">Model</h2>
        <div className="space-y-1.5">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="model"
              checked={modelSize === "4b"}
              onChange={() => { setModelSize("4b"); updateSetting("model_size", "4b"); }}
              className="accent-blue-500"
            />
            <span className="text-gray-700">Qwen3-4B (default)</span>
          </label>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="model"
              checked={modelSize === "1.7b"}
              onChange={() => { setModelSize("1.7b"); updateSetting("model_size", "1.7b"); }}
              className="accent-blue-500"
            />
            <span className="text-gray-700">Qwen3-1.7B (light)</span>
          </label>
        </div>
      </section>

      {/* Data */}
      <section className="mb-5">
        <h2 className="font-medium text-gray-600 mb-2">Data</h2>
        <div className="space-y-2">
          <button
            onClick={handleOpenDataFolder}
            className="px-3 py-1.5 bg-gray-100 text-gray-700 rounded text-xs hover:bg-gray-200 transition-colors"
          >
            Open Data Folder
          </button>
          <div>
            <button
              onClick={handleExport}
              className="px-3 py-1.5 bg-gray-100 text-gray-700 rounded text-xs hover:bg-gray-200 transition-colors"
            >
              Export JSON
            </button>
            {exportMsg && <span className="ml-2 text-xs text-green-600">{exportMsg}</span>}
          </div>
          <div>
            <div className="flex items-center gap-2 mb-1">
              <input
                type="text"
                className="px-2 py-1 border border-gray-200 rounded text-xs w-24 placeholder-gray-400"
                placeholder='Type DELETE'
                value={clearInput}
                onChange={(e) => { setClearInput(e.target.value); setClearMsg(""); }}
              />
              <button
                onClick={handleClearAll}
                className="px-3 py-1.5 bg-red-50 text-red-600 border border-red-200 rounded text-xs hover:bg-red-100 transition-colors"
              >
                Clear All Memory
              </button>
            </div>
            {clearMsg && <p className="text-xs text-red-500">{clearMsg}</p>}
          </div>
        </div>
      </section>

      {/* Indexing */}
      <section className="mb-5">
        <h2 className="font-medium text-gray-600 mb-2">Indexing</h2>
        <button
          onClick={handleToggleIndexing}
          className={`px-3 py-1.5 rounded text-xs transition-colors ${
            indexingPaused
              ? "bg-green-50 text-green-700 border border-green-200 hover:bg-green-100"
              : "bg-amber-50 text-amber-700 border border-amber-200 hover:bg-amber-100"
          }`}
        >
          {indexingPaused ? "Resume Indexing" : "Pause Indexing"}
        </button>
      </section>

      {/* Advanced */}
      <section>
        <h2 className="font-medium text-gray-600 mb-2">Advanced</h2>
        <button
          onClick={handleViewRedactionLog}
          className="px-3 py-1.5 bg-gray-100 text-gray-700 rounded text-xs hover:bg-gray-200 transition-colors"
        >
          View Redaction Log
        </button>
        {redactionLog !== null && (
          <pre className="mt-2 p-2 bg-gray-50 border border-gray-200 rounded text-xs max-h-40 overflow-y-auto whitespace-pre-wrap text-gray-600">
            {redactionLog}
          </pre>
        )}
      </section>
    </div>
  );
}
