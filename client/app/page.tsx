import Image from "next/image";
import Link from "next/link";
import { Server, Activity, ArrowRight, GitBranch, Mail, Layers, Terminal } from "lucide-react";
import JobList from "../components/JobList";

type RuntimeSummary = {
  service: string;
  status: string;
  total_jobs: number;
  running_jobs: number;
  completed_jobs: number;
};

async function loadRuntimeSummary(): Promise<RuntimeSummary | null> {
  const baseUrl = process.env.NEXT_PUBLIC_RUNTIME_URL ?? process.env.RUNTIME_URL ?? "http://127.0.0.1:8080";
  try {
    const response = await fetch(`${baseUrl}/public/summary`, { cache: "no-store" });
    if (!response.ok) return null;
    return (await response.json()) as RuntimeSummary;
  } catch {
    return null;
  }
}

export default async function Home() {
  const runtime = await loadRuntimeSummary();

  return (
    <div className="min-h-screen relative overflow-hidden text-zinc-900 dark:text-zinc-100">
      
      {/* Navbar */}
      <header className="mx-auto max-w-6xl px-6 py-6 flex items-center justify-between relative z-10">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-zinc-900 dark:bg-white flex items-center justify-center shadow-lg">
            <Layers className="w-6 h-6 text-white dark:text-zinc-900" />
          </div>
          <span className="font-bold tracking-tight text-xl">AI Edge Runtime</span>
        </div>
        <nav className="flex items-center gap-6">
          <a href="#features" className="text-sm font-medium hover:text-indigo-500 transition">Features</a>
          <a href="#how" className="text-sm font-medium hover:text-indigo-500 transition">Architecture</a>
          <Link href="/" className="text-sm font-medium px-4 py-2 rounded-full border border-zinc-200 dark:border-zinc-800 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition">
            Docs
          </Link>
        </nav>
      </header>

      {/* Hero Section */}
      <main className="mx-auto max-w-6xl px-6 py-20 relative z-10">
        <section className="grid grid-cols-1 lg:grid-cols-2 gap-16 items-center">
          <div className="animate-fade-in-up">
            <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-indigo-50 dark:bg-indigo-900/30 text-indigo-600 dark:text-indigo-400 text-xs font-semibold mb-6 border border-indigo-100 dark:border-indigo-800/50">
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-indigo-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-indigo-500"></span>
              </span>
              v0.1.0 Pre-release
            </div>
            <h1 className="text-5xl sm:text-6xl font-extrabold leading-[1.1] tracking-tight">
              Build and run isolated <span className="text-transparent bg-clip-text bg-gradient-to-r from-indigo-500 to-emerald-500">AI workloads</span> locally.
            </h1>
            <p className="mt-6 text-lg text-zinc-600 dark:text-zinc-400 max-w-xl leading-relaxed">
              Start by building the execution engine. Spawn processes, capture realtime logs, manage lifecycle, and easily evolve to containers and distributed scheduling.
            </p>

            <div className="mt-10 flex flex-wrap gap-4">
              <a
                href="#dashboard"
                className="inline-flex items-center gap-2 bg-zinc-900 dark:bg-white text-white dark:text-zinc-900 font-medium px-6 py-3 rounded-full shadow-xl shadow-zinc-900/20 dark:shadow-white/10 hover:scale-105 transition-transform"
              >
                Launch Dashboard <ArrowRight className="w-4 h-4" />
              </a>
              <a href="#how" className="inline-flex items-center gap-2 px-6 py-3 rounded-full border border-zinc-200 dark:border-zinc-800 font-medium hover:bg-zinc-50 dark:hover:bg-zinc-900 transition-colors">
                View Architecture
              </a>
            </div>
          </div>

          {/* Glassmorphic Stats Card */}
          <div className="glass-panel rounded-2xl p-8 animate-fade-in-up" style={{ animationDelay: '0.1s' }}>
            <div className="flex items-center justify-between mb-8">
              <div className="flex items-center gap-3">
                <Server className="w-5 h-5 text-zinc-500" />
                <span className="font-semibold tracking-tight text-lg">Cluster Status</span>
              </div>
              <div className={`flex items-center gap-2 text-xs font-medium px-3 py-1 rounded-full ${runtime?.status === "ok" ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20" : "bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20"}`}>
                <div className={`w-1.5 h-1.5 rounded-full ${runtime?.status === "ok" ? "bg-emerald-500" : "bg-amber-500"}`} />
                {runtime?.status === "ok" ? "Online" : "Offline"}
              </div>
            </div>
            
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-4 text-center">
              <div className="bg-white dark:bg-zinc-950 rounded-xl p-4 border border-zinc-100 dark:border-zinc-800 shadow-sm">
                <div className="text-3xl font-extrabold text-indigo-500">{runtime?.running_jobs ?? 0}</div>
                <div className="text-xs font-medium text-zinc-500 uppercase tracking-wider mt-1">Running</div>
              </div>
              <div className="bg-white dark:bg-zinc-950 rounded-xl p-4 border border-zinc-100 dark:border-zinc-800 shadow-sm">
                <div className="text-3xl font-extrabold text-emerald-500">{runtime?.completed_jobs ?? 0}</div>
                <div className="text-xs font-medium text-zinc-500 uppercase tracking-wider mt-1">Completed</div>
              </div>
              <div className="bg-white dark:bg-zinc-950 rounded-xl p-4 border border-zinc-100 dark:border-zinc-800 shadow-sm col-span-2 sm:col-span-1">
                <div className="text-3xl font-extrabold">{runtime?.total_jobs ?? 0}</div>
                <div className="text-xs font-medium text-zinc-500 uppercase tracking-wider mt-1">Total Jobs</div>
              </div>
            </div>

            <div className="mt-6 p-4 rounded-xl bg-zinc-950 border border-zinc-800 font-mono text-sm text-zinc-300 shadow-inner overflow-hidden relative group">
              <div className="absolute top-2 right-2 opacity-50 group-hover:opacity-100 transition-opacity">
                <Terminal className="w-4 h-4 text-zinc-500" />
              </div>
              <span className="text-indigo-400">POST</span> /execute<br/>
              {`{`}
              <br/>
              &nbsp;&nbsp;<span className="text-sky-300">"language"</span>: <span className="text-emerald-300">"python"</span>,<br/>
              &nbsp;&nbsp;<span className="text-sky-300">"code"</span>: <span className="text-emerald-300">"print('hello edge')"</span><br/>
              {`}`}
            </div>
          </div>
        </section>

        {/* Dashboard Section */}
        <section id="dashboard" className="mt-32 scroll-mt-24">
          <div className="flex flex-col items-center mb-10 text-center animate-fade-in-up">
            <h2 className="text-3xl font-bold tracking-tight">Runtime Dashboard</h2>
            <p className="text-zinc-500 mt-2">Submit workloads directly to your cluster and stream the results in realtime.</p>
          </div>
          <div className="animate-fade-in-up" style={{ animationDelay: '0.2s' }}>
            <JobList />
          </div>
        </section>

        {/* Features */}
        <section id="features" className="mt-32">
          <h2 className="text-3xl font-bold tracking-tight mb-8">Platform Features</h2>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
            <div className="glass-panel p-6 rounded-2xl">
              <div className="w-12 h-12 bg-indigo-500/10 rounded-xl flex items-center justify-center mb-4 text-indigo-500">
                <Terminal className="w-6 h-6" />
              </div>
              <h3 className="font-semibold text-lg">Multi-Language Runtime</h3>
              <p className="mt-2 text-sm text-zinc-500 dark:text-zinc-400 leading-relaxed">Securely execute Python, Node.js, or Bash scripts in isolated OS processes with enforced timeouts.</p>
            </div>
            <div className="glass-panel p-6 rounded-2xl">
              <div className="w-12 h-12 bg-emerald-500/10 rounded-xl flex items-center justify-center mb-4 text-emerald-500">
                <Activity className="w-6 h-6" />
              </div>
              <h3 className="font-semibold text-lg">Realtime Telemetry</h3>
              <p className="mt-2 text-sm text-zinc-500 dark:text-zinc-400 leading-relaxed">Stream standard output and error back to clients over bi-directional WebSocket connections instantly.</p>
            </div>
            <div className="glass-panel p-6 rounded-2xl">
              <div className="w-12 h-12 bg-amber-500/10 rounded-xl flex items-center justify-center mb-4 text-amber-500">
                <Layers className="w-6 h-6" />
              </div>
              <h3 className="font-semibold text-lg">Evolvable Architecture</h3>
              <p className="mt-2 text-sm text-zinc-500 dark:text-zinc-400 leading-relaxed">Start local, then seamlessly migrate your execution engine to Docker containers and Kubernetes.</p>
            </div>
          </div>
        </section>

      </main>

      <footer className="mt-24 border-t border-zinc-200 dark:border-zinc-800/50 bg-white/30 dark:bg-zinc-950/30 backdrop-blur-md">
        <div className="mx-auto max-w-6xl px-6 py-12 flex flex-col md:flex-row items-center justify-between gap-6">
          <div className="flex items-center gap-3">
            <Layers className="w-5 h-5 text-zinc-400" />
            <span className="font-medium text-zinc-500">AI Edge Runtime</span>
          </div>
          <div className="flex items-center gap-6 text-sm font-medium text-zinc-500">
            <a href="https://github.com/santhoshkumar0918" className="hover:text-zinc-900 dark:hover:text-white transition flex items-center gap-2">
              <GitBranch className="w-4 h-4" /> GitHub
            </a>
            <a href="mailto:krishnanramalingam87@gmail.com" className="hover:text-zinc-900 dark:hover:text-white transition flex items-center gap-2">
              <Mail className="w-4 h-4" /> Contact
            </a>
          </div>
        </div>
      </footer>
    </div>
  );
}
