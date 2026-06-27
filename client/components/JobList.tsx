"use client";

import React, { useEffect, useState } from "react";
import { Play, Square, Terminal as TerminalIcon, Clock, CheckCircle2, XCircle, Loader2, Key, RefreshCcw } from "lucide-react";

type Job = { id: string; status: string; created_at?: number | null };

type Logs = { stdout: string; stderr: string } | null;

const baseUrl = process.env.NEXT_PUBLIC_RUNTIME_URL ?? process.env.RUNTIME_URL ?? "http://127.0.0.1:8080";

export default function JobList() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [logs, setLogs] = useState<Record<string, Logs>>({});
  const [apiKey, setApiKey] = useState<string>("");
  const [sortBy, setSortBy] = useState<"created" | "status" | "id">("created");
  const [code, setCode] = useState<string>("print('hello edge runtime')\n");
  const [language, setLanguage] = useState<string>("python");
  const [timeoutMs, setTimeoutMs] = useState<number>(5000);
  const [wsStates, setWsStates] = useState<Record<string, "connecting" | "open" | "closed" | "error">>({});
  const [results, setResults] = useState<Record<string, { exit_code?: number | null; status?: string }>>({});
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [activeTab, setActiveTab] = useState<"submit" | "history">("submit");

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
      setError(`Connection error: ${msg}. Make sure backend is running at ${baseUrl}`);
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
      const res = await fetch(`${baseUrl}/jobs/${id}/logs?tail=20`, { cache: "no-store", headers });
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
    setIsSubmitting(true);
    try {
      const headers: Record<string,string> = { 'Content-Type': 'application/json' };
      const stored = localStorage.getItem("runtime_api_key") || apiKey;
      if (stored) headers["Authorization"] = `Bearer ${stored}`;

      const body = { language: language, code: code, timeout_ms: timeoutMs };
      const res = await fetch(`${baseUrl}/execute_async`, { method: 'POST', headers, body: JSON.stringify(body) });
      if (!res.ok) {
        const txt = await res.text();
        setError(`Failed to schedule: HTTP ${res.status} ${txt}`);
        setIsSubmitting(false);
        return;
      }
      const j = await res.json();
      const id = j.id as string;

      setJobs((s) => [{ id, status: 'running', created_at: Date.now() }, ...s]);
      setLogs((s) => ({ ...s, [id]: { stdout: '', stderr: '' } }));
      setActiveTab("history"); // Auto switch to history to watch it run

      setWsStates((s) => ({ ...s, [id]: 'connecting' }));
      const sock = wsForId(id);
      if (!sock) {
        setWsStates((s) => ({ ...s, [id]: 'error' }));
        setIsSubmitting(false);
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
          setJobs((s) => s.map((t) => (t.id === id ? { ...t, status: 'completed' } : t)));
          const m = msg.match(/exit=\s*(.*)$/);
          let exit_code: number | null | undefined = undefined;
          if (m && m[1]) {
            const digits = m[1].match(/-?\d+/);
            if (digits) {
              exit_code = Number(digits[0]);
            } else {
              exit_code = null;
            }
          }
          setResults((s) => ({ ...s, [id]: { exit_code, status: 'completed' } }));
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
            } finally {
              try { sock.close(); } catch (_) {}
              setWsStates((s) => ({ ...s, [id]: 'closed' }));
            }
          })();
        } else {
          setLogs((s) => ({ ...s, [id]: { stdout: (s[id]?.stdout || '') + msg + '\n', stderr: s[id]?.stderr || '' } }));
        }
      };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Submit error: ${msg}`);
    } finally {
      setIsSubmitting(false);
    }
  }

  useEffect(() => {
    loadJobs();
    const iv = setInterval(loadJobs, 5000);
    return () => clearInterval(iv);
  }, []);

  const getStatusIcon = (status: string) => {
    switch (status) {
      case "completed": return <CheckCircle2 className="w-4 h-4 text-emerald-500" />;
      case "failed": return <XCircle className="w-4 h-4 text-red-500" />;
      case "cancelled": return <Square className="w-4 h-4 text-amber-500" />;
      case "running": return <Loader2 className="w-4 h-4 text-indigo-500 animate-spin" />;
      default: return <Clock className="w-4 h-4 text-zinc-500" />;
    }
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case "completed": return "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-200 dark:border-emerald-900";
      case "failed": return "bg-red-500/10 text-red-700 dark:text-red-400 border-red-200 dark:border-red-900";
      case "cancelled": return "bg-amber-500/10 text-amber-700 dark:text-amber-400 border-amber-200 dark:border-amber-900";
      case "running": return "bg-indigo-500/10 text-indigo-700 dark:text-indigo-400 border-indigo-200 dark:border-indigo-900";
      default: return "bg-zinc-100 dark:bg-zinc-800 text-zinc-800 dark:text-zinc-200 border-zinc-200 dark:border-zinc-700";
    }
  };

  const sorted = [...jobs].sort((a,b) => {
    if (sortBy === "created") return (b.created_at || 0) - (a.created_at || 0);
    if (sortBy === "status") return a.status.localeCompare(b.status);
    return a.id.localeCompare(b.id);
  });

  return (
    <div className="w-full max-w-4xl mx-auto glass-panel rounded-2xl overflow-hidden shadow-2xl">
      {/* Tabs Header */}
      <div className="flex border-b border-zinc-200 dark:border-zinc-800">
        <button 
          onClick={() => setActiveTab("submit")}
          className={`flex-1 py-4 text-sm font-semibold transition-colors flex items-center justify-center gap-2 ${activeTab === 'submit' ? 'bg-zinc-50 dark:bg-zinc-900/80 text-indigo-600 dark:text-indigo-400 border-b-2 border-indigo-500' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-300'}`}
        >
          <TerminalIcon className="w-4 h-4" /> Code Editor
        </button>
        <button 
          onClick={() => setActiveTab("history")}
          className={`flex-1 py-4 text-sm font-semibold transition-colors flex items-center justify-center gap-2 ${activeTab === 'history' ? 'bg-zinc-50 dark:bg-zinc-900/80 text-indigo-600 dark:text-indigo-400 border-b-2 border-indigo-500' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-300'}`}
        >
          <Clock className="w-4 h-4" /> Job History ({jobs.length})
        </button>
      </div>

      <div className="p-6">
        {/* Error Banner */}
        {error && (
          <div className="mb-6 p-4 rounded-xl bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900/50 text-red-700 dark:text-red-400 text-sm flex items-start gap-3">
            <XCircle className="w-5 h-5 shrink-0 mt-0.5" />
            <div>
              <p className="font-semibold">Runtime Error</p>
              <p className="mt-1 opacity-90">{error}</p>
            </div>
          </div>
        )}

        {/* Submit Tab */}
        {activeTab === "submit" && (
          <div className="space-y-6 animate-fade-in">
            <div className="rounded-xl overflow-hidden border border-zinc-200 dark:border-zinc-800 shadow-sm">
              <div className="bg-zinc-100 dark:bg-zinc-900 px-4 py-3 flex items-center justify-between border-b border-zinc-200 dark:border-zinc-800">
                <div className="flex items-center gap-3">
                  <select 
                    value={language} 
                    onChange={(e) => {
                      const val = e.target.value;
                      setLanguage(val);
                      if (val === "javascript") setCode("console.log('hello edge from javascript');\n");
                      else if (val === "bash") setCode("echo 'hello edge from bash'\n");
                      else setCode("print('hello edge runtime')\n");
                    }} 
                    className="bg-white dark:bg-zinc-950 border border-zinc-300 dark:border-zinc-700 rounded-md text-xs font-medium px-3 py-1.5 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                  >
                    <option value="python">Python 3</option>
                    <option value="javascript">Node.js</option>
                    <option value="bash">Bash</option>
                  </select>
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-xs font-medium text-zinc-500">Timeout (ms):</span>
                  <input 
                    type="number" 
                    value={timeoutMs} 
                    onChange={(e) => setTimeoutMs(Number(e.target.value))} 
                    className="w-20 bg-white dark:bg-zinc-950 border border-zinc-300 dark:border-zinc-700 rounded-md text-xs px-2 py-1.5 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                  />
                </div>
              </div>
              <textarea 
                value={code} 
                onChange={(e) => setCode(e.target.value)} 
                className="w-full h-64 p-4 text-sm font-mono bg-zinc-50 dark:bg-zinc-950 text-zinc-800 dark:text-zinc-200 focus:outline-none resize-y"
                spellCheck={false}
              />
            </div>
            
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <div className="relative">
                  <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                    <Key className="w-4 h-4 text-zinc-400" />
                  </div>
                  <input
                    type="password"
                    placeholder="API Key (optional)"
                    className="pl-9 pr-3 py-2 bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 w-64 shadow-sm"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    onBlur={() => { if (apiKey) localStorage.setItem("runtime_api_key", apiKey); }}
                  />
                </div>
              </div>
              <button 
                onClick={submitCode}
                disabled={isSubmitting}
                className="flex items-center gap-2 px-6 py-2.5 bg-indigo-600 hover:bg-indigo-700 text-white font-medium rounded-lg shadow-lg shadow-indigo-600/20 transition-all disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {isSubmitting ? <Loader2 className="w-4 h-4 animate-spin" /> : <Play className="w-4 h-4 fill-current" />}
                Run Workload
              </button>
            </div>
          </div>
        )}

        {/* History Tab */}
        {activeTab === "history" && (
          <div className="space-y-4 animate-fade-in">
            <div className="flex items-center justify-between mb-6">
              <div className="flex items-center gap-3">
                <span className="text-sm font-medium text-zinc-500">Sort by:</span>
                <select 
                  value={sortBy} 
                  onChange={(e) => setSortBy(e.target.value as any)} 
                  className="bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 rounded-md text-sm px-3 py-1.5 focus:outline-none focus:ring-2 focus:ring-indigo-500"
                >
                  <option value="created">Newest First</option>
                  <option value="status">Status</option>
                  <option value="id">Job ID</option>
                </select>
              </div>
              <button 
                onClick={loadJobs}
                className="flex items-center gap-2 text-sm text-zinc-500 hover:text-zinc-900 dark:hover:text-white transition"
              >
                <RefreshCcw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
                Refresh
              </button>
            </div>

            {!loading && jobs.length === 0 && (
              <div className="text-center py-16 px-6 rounded-2xl border border-dashed border-zinc-300 dark:border-zinc-800">
                <TerminalIcon className="w-12 h-12 text-zinc-300 dark:text-zinc-700 mx-auto mb-4" />
                <h3 className="text-lg font-medium text-zinc-900 dark:text-zinc-100">No workloads found</h3>
                <p className="text-sm text-zinc-500 mt-1">Submit your first code snippet to see it here.</p>
                <button 
                  onClick={() => setActiveTab("submit")}
                  className="mt-6 px-4 py-2 bg-zinc-900 dark:bg-white text-white dark:text-zinc-900 text-sm font-medium rounded-lg"
                >
                  Go to Editor
                </button>
              </div>
            )}

            <div className="space-y-4">
              {sorted.map((j) => (
                <div key={j.id} className="p-5 rounded-xl bg-white dark:bg-zinc-950/80 border border-zinc-200 dark:border-zinc-800/80 shadow-sm hover:shadow-md transition-shadow group">
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-3">
                      <div className={`px-2.5 py-1 rounded-full text-xs font-semibold flex items-center gap-1.5 border ${getStatusColor(j.status)}`}>
                        {getStatusIcon(j.status)}
                        <span className="capitalize">{j.status}</span>
                      </div>
                      {wsStates[j.id] && j.status === 'running' && (
                        <div className="text-xs text-indigo-500 flex items-center gap-1">
                          <span className="relative flex h-2 w-2 mr-1">
                            {wsStates[j.id] === 'open' && <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-indigo-400 opacity-75"></span>}
                            <span className={`relative inline-flex rounded-full h-2 w-2 ${wsStates[j.id] === 'open' ? 'bg-indigo-500' : 'bg-zinc-400'}`}></span>
                          </span>
                          Stream: {wsStates[j.id]}
                        </div>
                      )}
                    </div>
                    <div className="text-xs font-mono text-zinc-400 dark:text-zinc-500">ID: {j.id.slice(0, 8)}...</div>
                  </div>
                  
                  <div className="flex items-center gap-4 text-xs text-zinc-500 mb-4">
                    <div className="flex items-center gap-1">
                      <Clock className="w-3 h-3" />
                      {new Date(j.created_at || Date.now()).toLocaleTimeString()} - {new Date(j.created_at || Date.now()).toLocaleDateString()}
                    </div>
                  </div>

                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => loadLogs(j.id)}
                      className="text-xs font-medium px-3 py-1.5 bg-zinc-100 dark:bg-zinc-900 hover:bg-zinc-200 dark:hover:bg-zinc-800 rounded-md transition"
                    >
                      Fetch Full Logs
                    </button>
                    {j.status === "running" && (
                      <button
                        onClick={() => cancelJob(j.id)}
                        className="text-xs font-medium px-3 py-1.5 bg-red-50 dark:bg-red-900/30 text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-900/50 rounded-md transition"
                      >
                        Abort
                      </button>
                    )}
                  </div>

                  {logs[j.id] && (
                    <div className="mt-4 rounded-lg overflow-hidden border border-zinc-200 dark:border-zinc-800 bg-zinc-50 dark:bg-zinc-950 text-left">
                      <div className="px-3 py-2 bg-zinc-100 dark:bg-zinc-900 border-b border-zinc-200 dark:border-zinc-800 text-xs font-semibold text-zinc-500 uppercase tracking-wider flex items-center gap-2">
                        <TerminalIcon className="w-3.5 h-3.5" /> Output
                      </div>
                      <div className="p-3">
                        {logs[j.id]?.stdout && (
                          <div className="mb-2">
                            <pre className="text-[13px] font-mono text-zinc-700 dark:text-zinc-300 whitespace-pre-wrap break-words">{logs[j.id]!.stdout}</pre>
                          </div>
                        )}
                        {logs[j.id]?.stderr && (
                          <div className="mt-2 pt-2 border-t border-red-200/50 dark:border-red-900/30">
                            <pre className="text-[13px] font-mono text-red-600 dark:text-red-400 whitespace-pre-wrap break-words">{logs[j.id]!.stderr}</pre>
                          </div>
                        )}
                        {!logs[j.id]?.stdout && !logs[j.id]?.stderr && (
                          <div className="text-xs text-zinc-400 italic">No output captured.</div>
                        )}
                      </div>
                    </div>
                  )}

                  {results[j.id] && (
                    <div className="mt-3 text-xs bg-zinc-50 dark:bg-zinc-900/50 rounded-lg p-3 border border-zinc-100 dark:border-zinc-800">
                      <span className="font-semibold mr-2 text-zinc-700 dark:text-zinc-300">Exit Status:</span> 
                      <span className="font-mono">{results[j.id].exit_code ?? 'None'}</span>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
