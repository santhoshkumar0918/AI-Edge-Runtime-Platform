import Image from "next/image";
import Link from "next/link";

export default function Home(): JSX.Element {
  return (
    <div className="min-h-screen bg-gradient-to-b from-white via-zinc-50 to-zinc-100 dark:from-black dark:via-zinc-900 dark:to-black text-zinc-900 dark:text-zinc-100">
      <header className="mx-auto max-w-5xl px-6 py-8 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Image src="/next.svg" alt="logo" width={36} height={12} className="dark:invert" />
          <span className="font-semibold text-lg">AI Edge Runtime</span>
        </div>
        <nav className="flex items-center gap-4">
          <a href="#features" className="text-sm hover:underline">
            Features
          </a>
          <a href="#how" className="text-sm hover:underline">
            How it works
          </a>
          <Link href="/" className="text-sm rounded-full px-3 py-1 border">Docs</Link>
        </nav>
      </header>

      <main className="mx-auto max-w-5xl px-6 py-16">
        <section className="grid grid-cols-1 md:grid-cols-2 gap-10 items-center">
          <div>
            <h1 className="text-4xl sm:text-5xl font-extrabold leading-tight">Build and run isolated AI workloads locally — start with the runtime.</h1>
            <p className="mt-6 text-lg text-zinc-600 dark:text-zinc-300 max-w-xl">
              Learn platform engineering by building the execution engine first. Spawn processes, capture logs, manage lifecycle, and evolve
              to containers and distributed scheduling when you're ready.
            </p>

            <div className="mt-8 flex flex-wrap gap-3">
              <a
                href="#get-started"
                className="inline-flex items-center gap-2 bg-zinc-900 text-white px-4 py-2 rounded-md shadow hover:brightness-95"
              >
                Get started
              </a>
              <a href="#how" className="inline-flex items-center gap-2 px-4 py-2 rounded-md border text-sm">
                Learn the architecture
              </a>
            </div>
          </div>

          <div className="bg-white/60 dark:bg-zinc-900/60 p-6 rounded-xl shadow-sm">
            <pre className="bg-transparent p-0 m-0 text-sm leading-6 overflow-auto">{`// POST /execute
{
  "language": "python",
  "code": "print('hello')",
  "timeout_ms": 5000
}`}</pre>
          </div>
        </section>

        <section id="features" className="mt-16">
          <h2 className="text-2xl font-semibold">Key features</h2>
          <div className="mt-6 grid grid-cols-1 sm:grid-cols-3 gap-6">
            <div className="p-4 bg-white dark:bg-zinc-900 rounded-lg shadow-sm">
              <h3 className="font-medium">Local-first</h3>
              <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">Start by executing processes locally to learn runtime basics.</p>
            </div>

            <div className="p-4 bg-white dark:bg-zinc-900 rounded-lg shadow-sm">
              <h3 className="font-medium">Realtime logs</h3>
              <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">Stream stdout/stderr to clients using WebSockets or SSE.</p>
            </div>

            <div className="p-4 bg-white dark:bg-zinc-900 rounded-lg shadow-sm">
              <h3 className="font-medium">Evolvable</h3>
              <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">Grow from processes → containers → distributed runtime.</p>
            </div>
          </div>
        </section>

        <section id="how" className="mt-16">
          <h2 className="text-2xl font-semibold">How it works</h2>
          <ol className="mt-4 list-decimal list-inside space-y-2 text-sm text-zinc-600 dark:text-zinc-400">
            <li>Client submits code to API Gateway.</li>
            <li>Gateway forwards request to runtime service which creates an execution record.</li>
            <li>Runtime spawns a process (or container later) and captures stdout/stderr.</li>
            <li>Logs stream back to the client and results are stored in the database.</li>
          </ol>
        </section>

        <section id="get-started" className="mt-16 py-8 border-t border-zinc-200 dark:border-zinc-800">
          <h2 className="text-xl font-semibold">Ready to build the engine?</h2>
          <p className="mt-3 text-sm text-zinc-600 dark:text-zinc-400">Follow the repository's README to begin Phase 0: learn async Rust, Docker, and Linux internals.</p>
        </section>
      </main>

      <footer className="mt-24 border-t border-zinc-200 dark:border-zinc-800 py-8">
        <div className="mx-auto max-w-5xl px-6 text-sm text-zinc-600 dark:text-zinc-400 flex items-center justify-between">
          <div>© {new Date().getFullYear()} AI Edge Runtime</div>
          <div className="flex items-center gap-4">
            <a href="https://github.com/santhoshkumar0918" className="hover:underline">GitHub</a>
            <a href="mailto:krishnanramalingam87@gmail.com" className="hover:underline">Contact</a>
          </div>
        </div>
      </footer>
    </div>
  );
}
