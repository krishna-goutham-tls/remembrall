import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

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

interface MemoryType {
  id: number;
  name: string;
  family: string;
  decay_band: string;
}

interface Project {
  id: number;
  name: string;
  memory_count: number;
}

interface MemoryPage {
  memories: MemoryRow[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

type SortField = "recency" | "strength" | "type";
type DecayFilter = "all" | "strong" | "fading" | "archived";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const FAMILY_COLORS: Record<string, string> = {
  durable: "bg-blue-100 text-blue-700",
  operational: "bg-amber-100 text-amber-700",
  ephemeral: "bg-purple-100 text-purple-700",
};

function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen).replace(/\s+\S*$/, "") + "...";
}

function strengthBars(strength: number) {
  const filled = strength > 0.66 ? 3 : strength > 0.33 ? 2 : strength > 0 ? 1 : 0;
  return (
    <span className="inline-flex gap-0.5">
      {[1, 2, 3].map((i) => (
        <span
          key={i}
          className={`inline-block w-1.5 h-3 rounded-sm ${i <= filled ? "bg-gray-600" : "bg-gray-200"}`}
        />
      ))}
    </span>
  );
}

function formatDate(dateStr: string): string {
  if (!dateStr) return "";
  try {
    const d = new Date(dateStr + "Z");
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  } catch {
    return dateStr;
  }
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

function Sidebar({
  types,
  projects,
  selectedTypes,
  setSelectedTypes,
  selectedProjects,
  setSelectedProjects,
  dateFrom,
  setDateFrom,
  dateTo,
  setDateTo,
  decayFilter,
  setDecayFilter,
}: {
  types: MemoryType[];
  projects: Project[];
  selectedTypes: string[];
  setSelectedTypes: (v: string[]) => void;
  selectedProjects: string[];
  setSelectedProjects: (v: string[]) => void;
  dateFrom: string;
  setDateFrom: (v: string) => void;
  dateTo: string;
  setDateTo: (v: string) => void;
  decayFilter: DecayFilter;
  setDecayFilter: (v: DecayFilter) => void;
}) {
  const toggleType = (name: string) => {
    setSelectedTypes(
      selectedTypes.includes(name)
        ? selectedTypes.filter((t) => t !== name)
        : [...selectedTypes, name]
    );
  };

  const toggleProject = (name: string) => {
    setSelectedProjects(
      selectedProjects.includes(name)
        ? selectedProjects.filter((p) => p !== name)
        : [...selectedProjects, name]
    );
  };

  return (
    <div className="w-52 shrink-0 border-r border-gray-200 p-3 overflow-y-auto text-xs">
      {/* Type filter */}
      <div className="mb-4">
        <h3 className="font-semibold text-gray-600 mb-1.5">Type</h3>
        <ul className="space-y-1">
          {types.map((t) => (
            <li key={t.id}>
              <label className="flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={selectedTypes.includes(t.name)}
                  onChange={() => toggleType(t.name)}
                  className="accent-blue-500"
                />
                <span className="text-gray-700">{t.name}</span>
              </label>
            </li>
          ))}
        </ul>
      </div>

      {/* Project filter */}
      <div className="mb-4">
        <h3 className="font-semibold text-gray-600 mb-1.5">Project</h3>
        <ul className="space-y-1">
          {projects.map((p) => (
            <li key={p.id}>
              <label className="flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={selectedProjects.includes(p.name)}
                  onChange={() => toggleProject(p.name)}
                  className="accent-blue-500"
                />
                <span className="text-gray-700">{p.name}</span>
                <span className="text-gray-400">({p.memory_count})</span>
              </label>
            </li>
          ))}
        </ul>
      </div>

      {/* Date range */}
      <div className="mb-4">
        <h3 className="font-semibold text-gray-600 mb-1.5">Date Range</h3>
        <input
          type="date"
          className="w-full mb-1 px-1.5 py-1 border border-gray-200 rounded text-xs"
          value={dateFrom}
          onChange={(e) => setDateFrom(e.target.value)}
        />
        <input
          type="date"
          className="w-full px-1.5 py-1 border border-gray-200 rounded text-xs"
          value={dateTo}
          onChange={(e) => setDateTo(e.target.value)}
        />
      </div>

      {/* Decay state */}
      <div>
        <h3 className="font-semibold text-gray-600 mb-1.5">Decay State</h3>
        {(["all", "strong", "fading", "archived"] as DecayFilter[]).map((d) => (
          <label key={d} className="flex items-center gap-1.5 cursor-pointer mb-0.5">
            <input
              type="radio"
              name="decay"
              checked={decayFilter === d}
              onChange={() => setDecayFilter(d)}
              className="accent-blue-500"
            />
            <span className="text-gray-700 capitalize">{d}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Detail Panel
// ---------------------------------------------------------------------------

function DetailPanel({
  memory,
  onClose,
  onAction,
  typeNames,
}: {
  memory: MemoryRow;
  onClose: () => void;
  onAction: () => void;
  typeNames: string[];
}) {
  const [editing, setEditing] = useState(false);
  const [editText, setEditText] = useState(memory.summary_text);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const handleReinforce = async () => {
    await invoke("reinforce_memory", { id: memory.id });
    onAction();
  };

  const handleDelete = async () => {
    await invoke("delete_memory", { id: memory.id });
    onAction();
    onClose();
  };

  const handleReclassify = async (type_name: string) => {
    await invoke("reclassify_memory", { id: memory.id, type_name });
    onAction();
  };

  const handleSaveSummary = async () => {
    await invoke("edit_memory_summary", { id: memory.id, summary: editText });
    setEditing(false);
    onAction();
  };

  return (
    <div className="w-80 shrink-0 border-l border-gray-200 p-4 overflow-y-auto text-sm">
      <div className="flex items-center justify-between mb-3">
        <h2 className="font-semibold text-gray-800">Memory Detail</h2>
        <button onClick={onClose} className="text-gray-400 hover:text-gray-600 text-lg leading-none">
          &times;
        </button>
      </div>

      {/* Summary */}
      {editing ? (
        <div className="mb-3">
          <textarea
            className="w-full h-24 p-2 border border-gray-300 rounded text-sm resize-none"
            value={editText}
            onChange={(e) => setEditText(e.target.value)}
          />
          <div className="flex gap-2 mt-1">
            <button onClick={handleSaveSummary} className="px-2 py-0.5 bg-blue-500 text-white rounded text-xs">Save</button>
            <button onClick={() => setEditing(false)} className="px-2 py-0.5 bg-gray-200 text-gray-600 rounded text-xs">Cancel</button>
          </div>
        </div>
      ) : (
        <p className="text-gray-700 mb-1 whitespace-pre-wrap">{memory.summary_text}</p>
      )}

      {!editing && (
        <button
          onClick={() => { setEditing(true); setEditText(memory.summary_text); }}
          className="text-blue-500 text-xs mb-3 hover:underline"
        >
          Edit summary
        </button>
      )}

      {/* Metadata */}
      <div className="space-y-1.5 text-xs text-gray-600 mb-4">
        <div className="flex justify-between">
          <span className="text-gray-400">Type</span>
          <select
            className="border border-gray-200 rounded px-1 py-0.5 text-xs"
            value={memory.type_name}
            onChange={(e) => handleReclassify(e.target.value)}
          >
            {typeNames.map((n) => (
              <option key={n} value={n}>{n}</option>
            ))}
          </select>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-400">Project</span>
          <span>{memory.project_name || "None"}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-400">Scope</span>
          <span className="capitalize">{memory.scope}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-400">Family</span>
          <span className={`px-1.5 py-0.5 rounded text-[10px] ${FAMILY_COLORS[memory.family] || "bg-gray-100 text-gray-600"}`}>
            {memory.family}
          </span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-400">Strength</span>
          <span className="flex items-center gap-1">{strengthBars(memory.strength)} {memory.strength.toFixed(2)}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-400">Recalls</span>
          <span>{memory.recall_count}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-400">Created</span>
          <span>{formatDate(memory.created_at)}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-400">Last accessed</span>
          <span>{formatDate(memory.last_accessed)}</span>
        </div>
      </div>

      {/* Actions */}
      <div className="space-y-2">
        <button
          onClick={handleReinforce}
          className="w-full py-1.5 bg-amber-50 text-amber-700 border border-amber-200 rounded text-xs hover:bg-amber-100 transition-colors"
        >
          ★ Reinforce
        </button>

        {confirmDelete ? (
          <div className="text-xs">
            <p className="text-red-600 mb-1">Delete this memory permanently?</p>
            <div className="flex gap-2">
              <button onClick={handleDelete} className="px-2 py-1 bg-red-500 text-white rounded text-xs">Delete</button>
              <button onClick={() => setConfirmDelete(false)} className="px-2 py-1 bg-gray-200 text-gray-600 rounded text-xs">Cancel</button>
            </div>
          </div>
        ) : (
          <button
            onClick={() => setConfirmDelete(true)}
            className="w-full py-1.5 bg-red-50 text-red-600 border border-red-200 rounded text-xs hover:bg-red-100 transition-colors"
          >
            Delete
          </button>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// BrowseView (main)
// ---------------------------------------------------------------------------

export default function BrowseView() {
  const [types, setTypes] = useState<MemoryType[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [memories, setMemories] = useState<MemoryRow[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(0);

  const [selectedTypes, setSelectedTypes] = useState<string[]>([]);
  const [selectedProjects, setSelectedProjects] = useState<string[]>([]);
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [decayFilter, setDecayFilter] = useState<DecayFilter>("all");
  const [sortField, setSortField] = useState<SortField>("recency");

  const [searchQuery, setSearchQuery] = useState("");
  const [selectedMemory, setSelectedMemory] = useState<MemoryRow | null>(null);

  const PAGE_SIZE = 25;

  // Fetch filters on mount
  useEffect(() => {
    (async () => {
      try {
        const filters = await invoke<{ types: MemoryType[]; projects: Project[] }>("get_filters");
        setTypes(filters.types);
        setProjects(filters.projects);
      } catch (e) {
        console.error("Failed to fetch filters:", e);
      }
    })();
  }, []);

  // Fetch memories when filters/sort/page change
  const fetchMemories = useCallback(async () => {
    try {
      const decayState = decayFilter !== "all" ? decayFilter : undefined;
      const dateRange = dateFrom && dateTo ? [dateFrom, dateTo] : undefined;
      const result = await invoke<MemoryPage>("get_memories_page", {
        typeFilter: selectedTypes.length === 1 ? selectedTypes[0] : undefined,
        projectFilter: selectedProjects.length === 1 ? selectedProjects[0] : undefined,
        dateRange,
        decayState,
        sort: sortField,
        page,
        pageSize: PAGE_SIZE,
      });
      setMemories(result.memories);
      setTotal(result.total);
      setTotalPages(result.total_pages);
    } catch (e) {
      console.error("Failed to fetch memories:", e);
    }
  }, [selectedTypes, selectedProjects, dateFrom, dateTo, decayFilter, sortField, page]);

  useEffect(() => {
    fetchMemories();
  }, [fetchMemories]);

  const handleSearch = useCallback(async () => {
    if (!searchQuery.trim()) {
      fetchMemories();
      return;
    }
    try {
      const results = await invoke<MemoryRow[]>("search_fts5", { query: searchQuery.trim() });
      setMemories(results);
      setTotal(results.length);
      setTotalPages(1);
      setPage(1);
    } catch (e) {
      console.error("Search failed:", e);
    }
  }, [searchQuery, fetchMemories]);

  const refreshAfterAction = useCallback(() => {
    fetchMemories();
    if (selectedMemory) {
      // Re-fetch the selected memory to get updated data
      invoke<MemoryRow>("get_memory", { id: selectedMemory.id }).then(setSelectedMemory).catch(() => {});
    }
  }, [fetchMemories, selectedMemory]);

  const typeNames = types.map((t) => t.name);

  return (
    <div className="h-screen flex bg-white text-sm">
      {/* Sidebar */}
      <Sidebar
        types={types}
        projects={projects}
        selectedTypes={selectedTypes}
        setSelectedTypes={setSelectedTypes}
        selectedProjects={selectedProjects}
        setSelectedProjects={setSelectedProjects}
        dateFrom={dateFrom}
        setDateFrom={setDateFrom}
        dateTo={dateTo}
        setDateTo={setDateTo}
        decayFilter={decayFilter}
        setDecayFilter={setDecayFilter}
      />

      {/* Main area */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Search + sort bar */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200">
          <input
            type="text"
            className="flex-1 px-2.5 py-1.5 border border-gray-200 rounded text-sm focus:outline-none focus:border-blue-400 placeholder-gray-400"
            placeholder="Search memories..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleSearch(); }}
          />
          {(["recency", "strength", "type"] as SortField[]).map((s) => (
            <button
              key={s}
              onClick={() => { setSortField(s); setPage(1); }}
              className={`px-2 py-1 rounded text-xs capitalize ${sortField === s ? "bg-gray-800 text-white" : "bg-gray-100 text-gray-600 hover:bg-gray-200"}`}
            >
              {s}
            </button>
          ))}
        </div>

        {/* Memory list */}
        <div className="flex-1 overflow-y-auto">
          {memories.length === 0 ? (
            <p className="text-gray-400 text-center py-8 text-xs">No memories found.</p>
          ) : (
            <ul>
              {memories.map((m) => (
                <li
                  key={m.id}
                  onClick={() => setSelectedMemory(m)}
                  className={`px-3 py-2.5 border-b border-gray-100 cursor-pointer hover:bg-gray-50 transition-colors ${selectedMemory?.id === m.id ? "bg-blue-50" : ""}`}
                >
                  <p className="text-gray-800 text-sm leading-snug" style={{ display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>
                    {m.summary_text}
                  </p>
                  <div className="flex items-center gap-2 mt-1 text-xs">
                    <span className={`px-1.5 py-0.5 rounded ${FAMILY_COLORS[m.family] || "bg-gray-100 text-gray-600"}`}>
                      {m.type_name}
                    </span>
                    {m.project_name && (
                      <span className="text-gray-400">{truncate(m.project_name, 20)}</span>
                    )}
                    {strengthBars(m.strength)}
                    <span className="text-gray-400 ml-auto">{formatDate(m.last_accessed || m.created_at)}</span>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>

        {/* Pagination */}
        {totalPages > 1 && (
          <div className="flex items-center justify-center gap-3 px-3 py-2 border-t border-gray-200 text-xs text-gray-500">
            <button
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              className="px-2 py-1 rounded bg-gray-100 disabled:opacity-40 hover:bg-gray-200"
            >
              Prev
            </button>
            <span>Page {page} of {totalPages} ({total} memories)</span>
            <button
              disabled={page >= totalPages}
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              className="px-2 py-1 rounded bg-gray-100 disabled:opacity-40 hover:bg-gray-200"
            >
              Next
            </button>
          </div>
        )}
      </div>

      {/* Detail panel */}
      {selectedMemory && (
        <DetailPanel
          memory={selectedMemory}
          onClose={() => setSelectedMemory(null)}
          onAction={refreshAfterAction}
          typeNames={typeNames}
        />
      )}
    </div>
  );
}
