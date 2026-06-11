import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";

interface BackfillProgress {
  indexed: number;
  total: number;
  status: string;
}

const TOTAL_STEPS = 5;
const STORAGE_KEY = "remembrall-ftue-step";

function StepDots({ current, total }: { current: number; total: number }) {
  return (
    <div className="flex justify-center gap-2 py-3">
      {Array.from({ length: total }, (_, i) => (
        <span
          key={i}
          className={`w-2 h-2 rounded-full transition-colors ${
            i + 1 <= current ? "bg-blue-500" : "bg-gray-300"
          }`}
        />
      ))}
    </div>
  );
}

export default function FTUEWizard() {
  const [step, setStep] = useState(() => {
    const saved = localStorage.getItem(STORAGE_KEY);
    return saved ? parseInt(saved, 10) : 1;
  });

  const persistStep = (s: number) => {
    setStep(s);
    localStorage.setItem(STORAGE_KEY, String(s));
  };

  return (
    <div className="h-screen flex flex-col bg-white text-sm">
      <div className="flex-1 flex items-center justify-center p-6">
        {step === 1 && <StepFDA onDone={() => persistStep(2)} />}
        {step === 2 && <StepTools onDone={() => persistStep(3)} />}
        {step === 3 && <StepModelDownload onDone={() => persistStep(4)} />}
        {step === 4 && <StepMCPReg onDone={() => persistStep(5)} />}
        {step === 5 && <StepBackfill />}
      </div>
      <StepDots current={step} total={TOTAL_STEPS} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 1: Full Disk Access
// ---------------------------------------------------------------------------

function StepFDA({ onDone }: { onDone: () => void }) {
  const [granted, setGranted] = useState(false);
  const intervalRef = useRef<number | null>(null);

  const checkPermission = useCallback(async () => {
    try {
      const ok = await invoke<boolean>("check_fda_permission");
      if (ok) {
        setGranted(true);
        if (intervalRef.current) clearInterval(intervalRef.current);
      }
    } catch {
      // permission not yet granted
    }
  }, []);

  useEffect(() => {
    checkPermission();
    intervalRef.current = window.setInterval(checkPermission, 2000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [checkPermission]);

  useEffect(() => {
    if (granted) {
      const t = setTimeout(onDone, 800);
      return () => clearTimeout(t);
    }
  }, [granted, onDone]);

  const handleOpenPrefs = useCallback(async () => {
    await open("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles");
  }, []);

  return (
    <div className="text-center max-w-sm">
      <h2 className="text-lg font-semibold text-gray-800 mb-3">Full Disk Access</h2>
      <p className="text-gray-600 mb-4 leading-relaxed">
        Remembrall needs Full Disk Access to read conversation files from
        <code className="mx-0.5 text-xs bg-gray-100 px-1 rounded">~/.factory</code>,
        <code className="mx-0.5 text-xs bg-gray-100 px-1 rounded">~/.claude</code>, etc.
      </p>
      {granted ? (
        <p className="text-green-600 font-medium">Permission granted!</p>
      ) : (
        <button
          onClick={handleOpenPrefs}
          className="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors"
        >
          Open System Preferences
        </button>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 2: Tool Selection
// ---------------------------------------------------------------------------

function StepTools({ onDone }: { onDone: () => void }) {
  return (
    <div className="text-center max-w-sm">
      <h2 className="text-lg font-semibold text-gray-800 mb-3">Tool Selection</h2>
      <div className="text-left mb-4 space-y-2">
        <label className="flex items-center gap-2">
          <input type="checkbox" checked readOnly className="accent-blue-500" />
          <span className="text-gray-700">Droid</span>
        </label>
        {["Codex", "Claude Code", "Cursor"].map((tool) => (
          <label key={tool} className="flex items-center gap-2 opacity-50">
            <input type="checkbox" checked={false} readOnly className="accent-gray-300" />
            <span className="text-gray-400">{tool}</span>
            <span className="text-xs text-gray-300">Coming soon</span>
          </label>
        ))}
      </div>
      <button
        onClick={onDone}
        className="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors"
      >
        Continue
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 3: Model Download (placeholder)
// ---------------------------------------------------------------------------

function StepModelDownload({ onDone }: { onDone: () => void }) {
  return (
    <div className="text-center max-w-sm">
      <h2 className="text-lg font-semibold text-gray-800 mb-3">Model Download</h2>
      <div className="space-y-3 mb-4">
        <div>
          <p className="text-xs text-gray-500 mb-1">Classifier (Qwen3-4B)</p>
          <div className="w-full bg-gray-100 rounded-full h-2">
            <div className="bg-blue-400 h-2 rounded-full" style={{ width: "0%" }} />
          </div>
        </div>
        <div>
          <p className="text-xs text-gray-500 mb-1">Embedder (bge-base)</p>
          <div className="w-full bg-gray-100 rounded-full h-2">
            <div className="bg-blue-400 h-2 rounded-full" style={{ width: "0%" }} />
          </div>
        </div>
      </div>
      <button
        onClick={onDone}
        className="px-4 py-2 bg-gray-200 text-gray-600 rounded hover:bg-gray-300 transition-colors text-xs"
      >
        Skip
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 4: MCP Registration
// ---------------------------------------------------------------------------

function StepMCPReg({ onDone }: { onDone: () => void }) {
  const [status, setStatus] = useState<"loading" | "success" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");

  useEffect(() => {
    (async () => {
      try {
        const already = await invoke<boolean>("check_mcp_registered");
        if (already) {
          setStatus("success");
          return;
        }
        const result = await invoke<{ success: boolean }>("register_mcp");
        if (result.success) {
          setStatus("success");
        } else {
          setStatus("error");
        }
      } catch (e) {
        setStatus("error");
        setErrorMsg(String(e));
      }
    })();
  }, []);

  useEffect(() => {
    if (status === "success") {
      const t = setTimeout(onDone, 1000);
      return () => clearTimeout(t);
    }
  }, [status, onDone]);

  return (
    <div className="text-center max-w-sm">
      <h2 className="text-lg font-semibold text-gray-800 mb-3">MCP Registration</h2>
      {status === "loading" && <p className="text-gray-500">Connecting to Droid...</p>}
      {status === "success" && (
        <div>
          <span className="text-green-600 text-xl">&#10003;</span>
          <p className="text-green-600 font-medium mt-1">Droid is connected</p>
        </div>
      )}
      {status === "error" && (
        <div className="text-left">
          <span className="text-red-500 text-xl">&#10007;</span>
          <p className="text-red-600 font-medium mb-2">Auto-registration failed</p>
          <p className="text-xs text-gray-600 mb-2">Add this to <code className="bg-gray-100 px-1 rounded">~/.factory/mcp.json</code>:</p>
          <pre className="p-2 bg-gray-50 border border-gray-200 rounded text-xs overflow-x-auto whitespace-pre-wrap text-gray-600">
{`{
  "remembrall": {
    "command": "node",
    "args": ["~/.factory/mcp-remembrall/dist/server.js"]
  }
}`}
          </pre>
          <p className="text-xs text-gray-600 mt-2">Also add to <code className="bg-gray-100 px-1 rounded">~/.factory/AGENTS.md</code>:</p>
          <pre className="p-2 bg-gray-50 border border-gray-200 rounded text-xs overflow-x-auto whitespace-pre-wrap text-gray-600">
            - Remembrall recall tool: Use the `recall` MCP tool at the start of every new session to load project context.
          </pre>
          {errorMsg && <p className="text-xs text-red-400 mt-1">{errorMsg}</p>}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 5: Backfill
// ---------------------------------------------------------------------------

function StepBackfill() {
  const [progress, setProgress] = useState<BackfillProgress | null>(null);
  const intervalRef = useRef<number | null>(null);

  useEffect(() => {
    const fetchProgress = async () => {
      try {
        const p = await invoke<BackfillProgress>("get_backfill_progress");
        setProgress(p);
        if (p.status !== "running") {
          if (intervalRef.current) clearInterval(intervalRef.current);
        }
      } catch {
        // ignore
      }
    };
    fetchProgress();
    intervalRef.current = window.setInterval(fetchProgress, 2000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, []);

  const pct = progress && progress.total > 0
    ? Math.round((progress.indexed / progress.total) * 100)
    : 0;

  return (
    <div className="text-center max-w-sm">
      <h2 className="text-lg font-semibold text-gray-800 mb-3">Indexing your past sessions...</h2>
      <div className="w-full bg-gray-100 rounded-full h-2 mb-2">
        <div
          className="bg-blue-500 h-2 rounded-full transition-all"
          style={{ width: `${pct}%` }}
        />
      </div>
      {progress && (
        <p className="text-xs text-gray-500">
          {progress.indexed}/{progress.total} sessions
          {progress.status === "complete" && " — Done!"}
        </p>
      )}
    </div>
  );
}
