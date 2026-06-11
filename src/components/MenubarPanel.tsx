import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

interface MemoryRow {
  id: number;
  project_name: string;
  type_name: string;
  family: string;
  summary_text: string;
  keywords: string;
  scope: string;
  importance: number;
  strength: number;
  recall_count: number;
  source_tool: string;
  created_at: string;
  last_accessed: string;
}

interface BackfillProgress {
  indexed: number;
  total: number;
  status: string;
}

function Divider() {
  return <hr className="border-gray-200 my-2" />;
}

function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen) + "...";
}

export default function MenubarPanel() {
  const [query, setQuery] = useState("");
  const [recentMemories, setRecentMemories] = useState<MemoryRow[]>([]);
  const [searchResults, setSearchResults] = useState<MemoryRow[] | null>(null);
  const [toolStatus, setToolStatus] = useState<Record<string, string>>({});
  const [backfill, setBackfill] = useState<BackfillProgress | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchData = useCallback(async () => {
    try {
      const [memories, status, progress] = await Promise.all([
        invoke<MemoryRow[]>("get_recent_memories", { limit: 5 }),
        invoke<Record<string, string>>("get_tool_status"),
        invoke<BackfillProgress>("get_backfill_progress"),
      ]);
      setRecentMemories(memories);
      setToolStatus(status);
      setBackfill(progress);
    } catch (e) {
      console.error("Failed to fetch panel data:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    invoke("clear_new_memories").catch(() => {});
  }, [fetchData]);

  // Poll backfill progress while running
  useEffect(() => {
    if (backfill?.status !== "running") return;
    const interval = setInterval(async () => {
      try {
        const progress = await invoke<BackfillProgress>("get_backfill_progress");
        setBackfill(progress);
        if (progress.status !== "running") clearInterval(interval);
      } catch {
        clearInterval(interval);
      }
    }, 5000);
    return () => clearInterval(interval);
  }, [backfill?.status]);

  const handleSearch = useCallback(async () => {
    if (!query.trim()) {
      setSearchResults(null);
      return;
    }
    try {
      const results = await invoke<MemoryRow[]>("search_fts5", { query: query.trim() });
      setSearchResults(results);
    } catch (e) {
      console.error("Search failed:", e);
    }
  }, [query]);

  const openWindow = useCallback(async (label: string) => {
    const win = await WebviewWindow.getByLabel(label);
    if (win) {
      await win.show();
      await win.setFocus();
    }
  }, []);

  const handleQuit = useCallback(async () => {
    // Quit the entire app, not just close this window
    await invoke("quit_app");
  }, []);

  const displayMemories = searchResults ?? recentMemories;
  const isSearching = searchResults !== null;

  if (loading) {
    return (
      <div className="w-[400px] max-h-[600px] p-4 text-center text-sm text-gray-400">
        Loading...
      </div>
    );
  }

  return (
    <div className="w-[400px] max-h-[600px] overflow-y-auto bg-white flex flex-col text-sm">
      {/* Search bar */}
      <div className="px-3 pt-3 pb-1">
        <input
          type="text"
          className="w-full px-3 py-1.5 rounded-md border border-gray-200 text-sm focus:outline-none focus:border-blue-400 placeholder-gray-400"
          placeholder="Search memories..."
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            if (!e.target.value.trim()) setSearchResults(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleSearch();
          }}
        />
      </div>

      <Divider />

      {/* Recent / search results memories */}
      <div className="px-3 py-1 flex-1">
        {displayMemories.length === 0 ? (
          <p className="text-gray-400 text-center py-4 text-xs">
            {isSearching
              ? "No results found."
              : "Your brain is building its first impression..."}
          </p>
        ) : (
          <ul className="space-y-1">
            {displayMemories.map((m) => (
              <li
                key={m.id}
                className="cursor-pointer px-2 py-1.5 rounded hover:bg-gray-50 transition-colors"
                onClick={() => openWindow("browse")}
              >
                <span className="text-gray-700">{truncate(m.summary_text, 60)}</span>
                <span className="ml-2 text-xs text-gray-400">{m.type_name}</span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <Divider />

      {/* Tool status row */}
      <div className="px-3 py-1.5 flex items-center gap-3 text-xs">
        {Object.entries(toolStatus).map(([tool, status]) => (
          <span key={tool} className="flex items-center gap-1">
            <span
              className={`inline-block w-2 h-2 rounded-full ${
                status === "green"
                  ? "bg-green-500"
                  : status === "red"
                    ? "bg-red-500"
                    : "bg-gray-300"
              }`}
            />
            <span className={status === "coming_soon" ? "text-gray-400" : "text-gray-600"}>
              {tool.charAt(0).toUpperCase() + tool.slice(1)}
            </span>
            {status === "coming_soon" && (
              <span className="text-gray-300 text-[10px]">soon</span>
            )}
          </span>
        ))}
      </div>

      {/* Backfill progress */}
      {backfill?.status === "running" && (
        <div className="px-3 py-1.5">
          <div className="flex items-center justify-between text-xs text-gray-500 mb-1">
            <span>Indexing: {backfill.indexed}/{backfill.total} sessions</span>
          </div>
          <div className="w-full bg-gray-100 rounded-full h-1.5">
            <div
              className="bg-blue-500 h-1.5 rounded-full transition-all"
              style={{
                width: `${backfill.total > 0 ? (backfill.indexed / backfill.total) * 100 : 0}%`,
              }}
            />
          </div>
        </div>
      )}

      <Divider />

      {/* Links row */}
      <div className="px-3 py-2 flex items-center gap-4 text-xs">
        <button
          className="text-blue-600 hover:text-blue-800 transition-colors"
          onClick={() => openWindow("browse")}
        >
          Browse All
        </button>
        <button
          className="text-blue-600 hover:text-blue-800 transition-colors"
          onClick={() => openWindow("settings")}
        >
          Settings
        </button>
        <button
          className="text-gray-400 hover:text-gray-600 transition-colors ml-auto"
          onClick={handleQuit}
        >
          Quit
        </button>
      </div>
    </div>
  );
}
