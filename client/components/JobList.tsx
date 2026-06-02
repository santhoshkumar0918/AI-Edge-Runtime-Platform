"use client";

import React, { useEffect, useState } from "react";

type Job = { id: string; status: string; seenAt: number };

type Logs = { stdout: string; stderr: string } | null;

const baseUrl = process.env.NEXT_PUBLIC_RUNTIME_URL ?? process.env.RUNTIME_URL ?? "http://127.0.0.1:8081";

export default function JobList() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(false);
  const [logs, setLogs] = useState<Record<string, Logs>>({});
  const [apiKey, setApiKey] = useState<string>("{}");
  const [sortBy, setSortBy] = useState<"seen" | "status" | "id">("seen");

  async function loadJobs() {
    setLoading(true);
    try {
      const headers: Record<string,string> = {};
      const stored = localStorage.getItem("runtime_api_key") || apiKey;
      if (stored) {
        headers["Authorization"] = `Bearer ${stored}`;
      }
      const res = await fetch(`${baseUrl}/jobs`, { cache: "no-store", headers });
      if (!res.ok) {
        setJobs([]);
        return;
      }
      const body = await res.json();
      const incoming: Job[] = (body.jobs || []).map((j: any) => ({ id: j.id, status: j.status, seenAt: Date.now() }));
      // merge with existing seenAt if present
      const seenMap: Record<string, number> = {};
      jobs.forEach((x) => (seenMap[x.id] = x.seenAt));
      const merged = incoming.map((x) => ({ ...x, seenAt: seenMap[x.id] || x.seenAt }));
      setJobs(merged);
    } catch (e) {
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
      const res = await fetch(`${baseUrl}/jobs/${id}/logs?tail=5`, { cache: "no-store", headers });
      if (!res.ok) {
        setLogs((s) => ({ ...s, [id]: { stdout: "", stderr: "failed to load" } }));
        return;
      }
      const body = await res.json();
      setLogs((s) => ({ ...s, [id]: { stdout: body.stdout || "", stderr: body.stderr || "" } }));
    } catch (e) {
      setLogs((s) => ({ ...s, [id]: { stdout: "", stderr: "error" } }));
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
      } else {
        console.error("failed to cancel", await res.text());
      }
    } catch (e) {
      console.error(e);
    }
  }

  useEffect(() => {
    loadJobs();
    const iv = setInterval(loadJobs, 5000);
    return () => clearInterval(iv);
  }, []);

  const sorted = [...jobs].sort((a,b) => {
    if (sortBy === "seen") return b.seenAt - a.seenAt;
    if (sortBy === "status") return a.status.localeCompare(b.status);
    return a.id.localeCompare(b.id);
  });

  return (
    <div className="mt-8">
      <h3 className="text-lg font-medium">Recent jobs</h3>
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
            <option value="seen">Newest</option>
            <option value="status">Status</option>
            <option value="id">ID</option>
          </select>
        </div>

        {loading && <div className="text-sm text-zinc-500">Loading...</div>}
        {!loading && jobs.length === 0 && <div className="text-sm text-zinc-500">No jobs yet</div>}
        {sorted.map((j) => (
          <div key={j.id} className="p-3 border rounded-md bg-white dark:bg-zinc-900 flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <div className="text-sm text-zinc-700 dark:text-zinc-200">{j.id}</div>
              <div className="text-xs px-2 py-1 rounded-full bg-zinc-100 dark:bg-zinc-800">{j.status}</div>
            </div>
            <div className="text-xs text-zinc-500">Seen: {new Date(j.seenAt).toLocaleString()}</div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => loadLogs(j.id)}
                className="text-sm px-2 py-1 border rounded text-zinc-700 dark:text-zinc-200"
              >
                View logs
              </button>
              <button
                onClick={() => cancelJob(j.id)}
                className="text-sm px-2 py-1 border rounded text-red-600"
              >
                Cancel
              </button>
              <a href={`${baseUrl}/status/${j.id}`} className="text-sm text-zinc-500 hover:underline">Status</a>
            </div>
            {logs[j.id] && (
              <div className="mt-2 text-xs bg-zinc-50 dark:bg-zinc-950 p-2 rounded">
                <div className="font-medium">Stdout</div>
                <pre className="text-xs whitespace-pre-wrap">{logs[j.id]!.stdout || "(empty)"}</pre>
                <div className="font-medium mt-2">Stderr</div>
                <pre className="text-xs whitespace-pre-wrap">{logs[j.id]!.stderr || "(empty)"}</pre>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
