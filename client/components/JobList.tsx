"use client";

import React, { useEffect, useState } from "react";

type Job = { id: string; status: string; created_at?: number | null };

type Logs = { stdout: string; stderr: string } | null;

const baseUrl = process.env.NEXT_PUBLIC_RUNTIME_URL ?? process.env.RUNTIME_URL ?? "http://127.0.0.1:8081";

export default function JobList() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [logs, setLogs] = useState<Record<string, Logs>>({});
  const [apiKey, setApiKey] = useState<string>("");
  const [sortBy, setSortBy] = useState<"created" | "status" | "id">("created");
  const [code, setCode] = useState<string>("print('hello from runtime')\n");
  const [language, setLanguage] = useState<string>("python");
  const [timeoutMs, setTimeoutMs] = useState<number>(5000);
  const [wsStates, setWsStates] = useState<Record<string, "connecting" | "open" | "closed" | "error">>({});
  const [results, setResults] = useState<Record<string, { exit_code?: number | null; status?: string }>>({});

  async function loadJobs() {
    setLoading(true);
    setError(null);
    try {
      const headers: Record<string,string> = {};
      const stored = localStorage.getItem("runtime_api_key") || apiKey;
      if (stored) {
        headers["Authorization"] = `Bearer ${stored}`;
      }
      const res = await fetch(`${baseUrl}/jobs`, { cache: "no-store", headers });
      if (!res.ok) {
        if (res.status === 401) {
          setError("Unauthorized: invalid API key");
        } else if (res.status === 403) {
          setError("Forbidden: access denied");
        } else {
          setError(`Failed to load jobs (HTTP ${res.status})`);
        }
        setJobs([]);
        return;
      }
      const body = await res.json();
      const incoming: Job[] = (body.jobs || []).map((j: any) => ({ id: j.id, status: j.status, created_at: j.created_at ?? Date.now() }));
      setJobs(incoming);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Connection error: ${msg}`);
      setJobs([]);
    } finally {
      setLoading(false);
    }
  }

  async function loadLogs(id: string) {
    try {
      const headers: Record<string,string> = {};
      const stored = localStorage.getItem("runtime_api_key") || apiKey;
      if (stored) {
        headers["Authorization"] = `Bearer ${stored}`;
      }
      const res = await fetch(`${baseUrl}/jobs/${id}/logs?tail=10`, { cache: "no-store", headers });
      if (!res.ok) {
        setLogs((s) => ({ ...s, [id]: { stdout: "", stderr: `Error: HTTP ${res.status}` } }));
        return;
      }
      const body = await res.json();
      setLogs((s) => ({ ...s, [id]: { stdout: body.stdout || "", stderr: body.stderr || "" } }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : "error";
      setLogs((s) => ({ ...s, [id]: { stdout: "", stderr: msg } }));
    }
  }

  async function cancelJob(id: string) {
    try {
      const headers: Record<string,string> = {};
      const stored = localStorage.getItem("runtime_api_key") || apiKey;
      if (stored) {
        headers["Authorization"] = `Bearer ${stored}`;
      }
      const res = await fetch(`${baseUrl}/jobs/${id}`, { method: "DELETE", headers });
      if (res.ok) {
        setJobs((s) => s.map((j) => (j.id === id ? { ...j, status: "cancelled" } : j)));
        setError(null);
      } else {
        const text = await res.text();
        setError(`Failed to cancel job: ${text}`);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Cancel error: ${msg}`);
    }
  }

  function wsForId(id: string): WebSocket | null {
    try {
      const full = `${baseUrl}/ws/${id}`;
      const u = new URL(full);
      u.protocol = u.protocol === "https:" ? "wss:" : "ws:";
      return new WebSocket(u.toString());
    } catch (e) {
      return null;
    }
  }

  async function submitCode() {
    setError(null);
    try {
      const headers: Record<string,string> = { 'Content-Type': 'application/json' };
      const stored = localStorage.getItem("runtime_api_key") || apiKey;
      if (stored) headers["Authorization"] = `Bearer ${stored}`;

      const body = { language: language, code: code, timeout_ms: timeoutMs };
      const res = await fetch(`${baseUrl}/execute_async`, { method: 'POST', headers, body: JSON.stringify(body) });
      if (!res.ok) {
        const txt = await res.text();
        setError(`Failed to schedule: HTTP ${res.status} ${txt}`);
        return;
      }
      const j = await res.json();
      const id = j.id as string;

      // optimistically add job
      setJobs((s) => [{ id, status: 'running', created_at: Date.now() }, ...s]);
      setLogs((s) => ({ ...s, [id]: { stdout: '', stderr: '' } }));

      // open websocket to stream logs
      setWsStates((s) => ({ ...s, [id]: 'connecting' }));
      const sock = wsForId(id);
      if (!sock) {
        setWsStates((s) => ({ ...s, [id]: 'error' }));
        return;
      }
      sock.onopen = () => {
        setWsStates((s) => ({ ...s, [id]: 'open' }));
      };
      sock.onclose = () => {
        setWsStates((s) => ({ ...s, [id]: 'closed' }));
      };
      sock.onerror = () => {
        setWsStates((s) => ({ ...s, [id]: 'error' }));
      };

      sock.onmessage = (ev) => {
        const msg = ev.data as string;
        if (msg.startsWith('OUT:')) {
          const line = msg.slice(4).trim();
          setLogs((s) => ({ ...s, [id]: { stdout: (s[id]?.stdout || '') + line + '\n', stderr: s[id]?.stderr || '' } }));
        } else if (msg.startsWith('ERR:')) {
          const line = msg.slice(4).trim();
          setLogs((s) => ({ ...s, [id]: { stdout: s[id]?.stdout || '', stderr: (s[id]?.stderr || '') + line + '\n' } }));
        } else if (msg.startsWith('DONE:')) {
          // finalization message — mark job completed
          setJobs((s) => s.map((t) => (t.id === id ? { ...t, status: 'completed' } : t)));
          // try to parse exit code from message like: "DONE: exit=Some(0)" or "DONE: exit=None"
          const m = msg.match(/exit=\s*(.*)$/);
          let exit_code: number | null | undefined = undefined;
          if (m && m[1]) {
            const digits = m[1].match(/-?\\d+/);
            if (digits) {
              exit_code = Number(digits[0]);
            } else {
              exit_code = null;
            }
          }
          setResults((s) => ({ ...s, [id]: { exit_code, status: 'completed' } }));
          // fetch final logs/result from the API to ensure full output
          (async () => {
            try {
              const headers: Record<string,string> = {};
              const stored = localStorage.getItem("runtime_api_key") || apiKey;
              if (stored) headers["Authorization"] = `Bearer ${stored}`;
              const r = await fetch(`${baseUrl}/status/${id}`, { cache: 'no-store', headers });
              if (r.ok) {
                const body = await r.json();
                if (body && typeof body === 'object') {
                  setResults((s) => ({ ...s, [id]: { exit_code: body.exit_code ?? exit_code, status: body.status } }));
                }
              }
              const logsRes = await fetch(`${baseUrl}/jobs/${id}/logs?tail=0`, { cache: 'no-store', headers });
              if (logsRes.ok) {
                const logsBody = await logsRes.json();
                setLogs((s) => ({ ...s, [id]: { stdout: logsBody.stdout || '', stderr: logsBody.stderr || '' } }));
              }
            } catch (e) {
              // ignore
            } finally {
              try { sock.close(); } catch (_) {}
              setWsStates((s) => ({ ...s, [id]: 'closed' }));
            }
          })();
        } else {
          // generic text
          setLogs((s) => ({ ...s, [id]: { stdout: (s[id]?.stdout || '') + msg + '\n', stderr: s[id]?.stderr || '' } }));
        }
      };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Submit error: ${msg}`);
    }
  }

  useEffect(() => {
    loadJobs();
    const iv = setInterval(loadJobs, 5000);
    return () => clearInterval(iv);
  }, []);

  const getStatusColor = (status: string) => {
    switch (status) {
      case "completed": return "bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200";
      case "failed": return "bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200";
      case "cancelled": return "bg-yellow-100 dark:bg-yellow-900 text-yellow-800 dark:text-yellow-200";
      case "running": return "bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200";
      default: return "bg-zinc-100 dark:bg-zinc-800 text-zinc-800 dark:text-zinc-200";
    }
  };

  const sorted = [...jobs].sort((a,b) => {
    if (sortBy === "created") return (b.created_at || 0) - (a.created_at || 0);
    if (sortBy === "status") return a.status.localeCompare(b.status);
    return a.id.localeCompare(b.id);
  });

  return (
    <div className="mt-8">
      <h3 className="text-lg font-medium">Recent jobs</h3>
      <div className="mt-4 p-3 border rounded-md bg-white dark:bg-zinc-900">
        <div className="flex flex-col gap-2">
          <label className="text-sm font-medium">Code</label>
          <textarea value={code} onChange={(e) => setCode(e.target.value)} className="w-full h-28 p-2 border rounded text-sm font-mono bg-zinc-50 dark:bg-zinc-950" />
          <div className="flex items-center gap-2">
            <label className="text-sm">Language</label>
            <select value={language} onChange={(e) => setLanguage(e.target.value)} className="px-2 py-1 border rounded text-sm">
              <option value="python">python</option>
            </select>
            <label className="text-sm">Timeout (ms)</label>
            <input type="number" value={timeoutMs} onChange={(e) => setTimeoutMs(Number(e.target.value))} className="px-2 py-1 border rounded text-sm w-28" />
            <button onClick={submitCode} className="ml-auto text-sm px-3 py-1 bg-zinc-900 text-white rounded">Run</button>
          </div>
        </div>
      </div>
      {error && <div className="mt-3 p-3 bg-red-50 border border-red-200 rounded text-red-700 text-sm">{error}</div>}
      <div className="mt-3 space-y-2">
        <div className="flex items-center gap-2">
          <input
            placeholder="API key (optional)"
            className="px-2 py-1 border rounded text-sm"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            onBlur={() => {
              if (apiKey) localStorage.setItem("runtime_api_key", apiKey);
            }}
          />
          <button
            onClick={() => { localStorage.removeItem("runtime_api_key"); setApiKey(""); }}
            className="text-sm px-2 py-1 border rounded"
          >
            Clear
          </button>
          <label className="text-sm">Sort:</label>
          <select value={sortBy} onChange={(e) => setSortBy(e.target.value as any)} className="text-sm px-2 py-1 border rounded">
            <option value="created">Newest</option>
            <option value="status">Status</option>
            <option value="id">ID</option>
          </select>
        </div>

        {loading && <div className="text-sm text-zinc-500 animate-pulse">Loading jobs...</div>}
        {!loading && jobs.length === 0 && <div className="text-sm text-zinc-500">No jobs yet</div>}
        {sorted.map((j) => (
          <div key={j.id} className="p-3 border rounded-md bg-white dark:bg-zinc-900 flex flex-col gap-2 hover:shadow-sm transition">
            <div className="flex items-center justify-between">
              <div className="text-sm font-mono text-zinc-700 dark:text-zinc-200 truncate">{j.id}</div>
              <div className="flex items-center gap-2">
                <div className={`text-xs px-2 py-1 rounded-full font-medium ${getStatusColor(j.status)}`}>{j.status}</div>
                {wsStates[j.id] && j.status === 'running' && (
                  <div className={`text-xs px-2 py-1 rounded-full font-medium ${wsStates[j.id] === 'open' ? 'bg-emerald-100 text-emerald-700' : wsStates[j.id] === 'connecting' ? 'bg-amber-100 text-amber-700' : 'bg-zinc-100 text-zinc-700'}`}>
                    {wsStates[j.id]}
                  </div>
                )}
              </div>
            </div>
            <div className="text-xs text-zinc-500">Created: {new Date(j.created_at || Date.now()).toLocaleString()}</div>
            <div className="flex items-center gap-2 flex-wrap">
              <button
                onClick={() => loadLogs(j.id)}
                className="text-sm px-2 py-1 border rounded text-zinc-700 dark:text-zinc-200 hover:bg-zinc-50 dark:hover:bg-zinc-800 transition"
              >
                View logs
              </button>
              {j.status === "running" && (
                <button
                  onClick={() => cancelJob(j.id)}
                  className="text-sm px-2 py-1 border border-red-300 rounded text-red-600 hover:bg-red-50 dark:hover:bg-red-900 transition"
                >
                  Cancel
                </button>
              )}
              <a href={`${baseUrl}/status/${j.id}`} className="text-sm text-zinc-500 hover:underline">Details</a>
            </div>
            {logs[j.id] && (
              <div className="mt-2 text-xs bg-zinc-50 dark:bg-zinc-950 p-3 rounded border border-zinc-200 dark:border-zinc-800">
                <div className="font-medium mb-1 text-zinc-700 dark:text-zinc-300">Stdout</div>
                <pre className="text-xs whitespace-pre-wrap break-words max-h-24 overflow-y-auto">{logs[j.id]!.stdout || "(empty)"}</pre>
                <div className="font-medium mt-2 mb-1 text-zinc-700 dark:text-zinc-300">Stderr</div>
                <pre className="text-xs whitespace-pre-wrap break-words max-h-24 overflow-y-auto">{logs[j.id]!.stderr || "(empty)"}</pre>
              </div>
            )}
            {results[j.id] && (
              <div className="mt-2 text-sm rounded p-3 border border-zinc-200 bg-zinc-50 dark:bg-zinc-950 dark:border-zinc-800">
                <div className="font-medium text-zinc-700 dark:text-zinc-300">Result</div>
                <div className="text-xs text-zinc-600 dark:text-zinc-400 mt-1">Status: {results[j.id].status}</div>
                <div className="text-xs text-zinc-600 dark:text-zinc-400">Exit code: {results[j.id].exit_code ?? 'N/A'}</div>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
