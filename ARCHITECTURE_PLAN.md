# AI Edge Runtime Platform - Comprehensive Architecture Plan

## 📋 Document Purpose

This document explains:
- **Why** we build in this order
- **What** lives in each folder
- **How** services communicate
- **What concepts** you learn at each stage
- **The execution flow** end-to-end

This is a reference document for the entire learning journey.

---

## 🎯 The Core Question We're Answering

**"How does a cloud platform execute user code safely?"**

This is not a UI question. Not an AI question. Not a DevOps question.

It's a **systems design question**.

The answer you build will teach you platform engineering from first principles.

---

## 📊 Phase Overview

| Phase | Goal | What You Learn | Duration |
|-------|------|----------------|----------|
| **0** | Foundation knowledge | Async Rust, Docker basics, Linux concepts | ~2 weeks |
| **1** | Local runtime execution | Process management, lifecycle control, streaming | ~2 weeks |
| **2** | Containerized execution | Isolation, resource limits, container APIs | ~2 weeks |
| **3** | Distributed multi-node | Scheduling, queues, coordination | ~3 weeks |
| **4** | Kubernetes integration | Orchestration, deployments, operators | ~2 weeks |
| **5** | AI runtime layer | Agent execution, memory, workflows | ~3 weeks |

---

# 🏗️ INITIAL MONOREPO STRUCTURE

## The Philosophy

**Start small, grow intentionally.**

Do NOT build the massive 50-folder structure shown in the main README yet.

That's the *end goal*, not the starting point.

Here's what we actually need initially:

```
ai-edge-runtime/
│
├── apps/
│   └── dashboard-web/                    # Phase 3+: UI for the platform
│
├── services/
│   ├── api-gateway/                      # Phase 1: Entry point
│   └── runtime-service/                  # Phase 1: Core execution engine
│
├── packages/
│   └── shared-types/                     # Phase 0: Shared Rust types + TypeScript types
│
├── infrastructure/
│   ├── docker/
│   │   ├── runtime/                      # Phase 2: Container for executing workloads
│   │   └── services/                     # Phase 2: Containers for our services
│   └── kubernetes/                       # Phase 4: K8s manifests
│
├── scripts/
│   ├── dev/
│   │   └── setup.sh                      # Phase 0: Local dev setup
│   └── testing/
│       └── test-execution.sh             # Phase 1: Test the runtime
│
├── docs/
│   ├── ARCHITECTURE.md                   # How everything connects
│   ├── CONCEPTS.md                       # Async Rust, Docker, Linux concepts
│   ├── LEARNING_PATH.md                  # What to study before each phase
│   └── API.md                            # API gateway endpoints
│
├── .github/
│   └── workflows/                        # Phase 3+: CI/CD pipelines
│
├── docker-compose.yml                    # Phase 1: Local dev environment
├── Cargo.toml                            # Phase 0: Root workspace
├── ARCHITECTURE_PLAN.md                  # This file
├── README.md                             # Updated with new structure
└── DEVELOPMENT_ROADMAP.md               # Step-by-step what to build next
```

---

## 🧭 Why This Structure?

### `apps/dashboard-web/`
- **What:** Next.js frontend (we already have it)
- **When created:** Phase 1 (but minimal UI until Phase 3)
- **Purpose:** Dashboard to visualize executions, logs, metrics
- **Initially:** Just has the shell, mostly waits for backend APIs

### `services/`
- **What:** All backend services (Rust)
- **Why separate:** Each service has its own Cargo.toml, dependencies, tests
- **Communication:** HTTP/gRPC between services (explained below)

#### `services/api-gateway/`
**The front door of our platform.**

```
services/api-gateway/
├── src/
│   ├── main.rs                 # Tokio runtime initialization
│   ├── handlers/
│   │   ├── execute.rs          # POST /execute endpoint
│   │   ├── get_execution.rs    # GET /execution/:id endpoint
│   │   └── logs.rs             # Logs streaming (WebSocket/SSE)
│   ├── middleware/
│   │   ├── auth.rs             # Future: JWT validation
│   │   └── logging.rs          # Request/response logging
│   ├── client/
│   │   └── runtime_client.rs   # HTTP client to runtime-service
│   └── error.rs                # Error handling
├── Cargo.toml                  # Dependencies: axum, tokio, serde, etc.
├── Dockerfile                  # How to containerize this service
└── tests/
    └── integration_tests.rs    # Test API endpoints
```

**What it does:**
1. Receives requests: `POST /execute { language: "python", code: "..." }`
2. Validates input
3. Generates unique execution ID
4. Calls runtime-service to execute
5. Streams back logs in real-time
6. Returns execution result

**Why it exists:**
- Single entry point (not calling runtime directly)
- Easier to add auth, rate limiting later
- Can scale independently

**Concepts you learn:**
- Axum web framework patterns
- Request/response handling
- HTTP client development
- Error propagation in Rust
- Async request handlers

#### `services/runtime-service/`
**The actual execution engine. This is the SOUL.**

```
services/runtime-service/
├── src/
│   ├── main.rs                 # Tokio runtime + server
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── process.rs          # Phase 1: Execute as local process
│   │   ├── container.rs        # Phase 2: Execute inside Docker
│   │   └── sandbox.rs          # Phase 3: Execute in WASM/Firecracker
│   ├── lifecycle/
│   │   ├── mod.rs
│   │   ├── startup.rs          # Start execution
│   │   ├── monitoring.rs       # Monitor while running
│   │   └── cleanup.rs          # Kill and clean up
│   ├── logs/
│   │   ├── mod.rs
│   │   ├── capture.rs          # Capture stdout/stderr
│   │   ├── stream.rs           # Stream to client
│   │   └── store.rs            # Persist to database
│   ├── models/
│   │   ├── execution.rs        # Execution struct
│   │   └── workload.rs         # Workload definition
│   ├── db/
│   │   ├── mod.rs
│   │   ├── schema.rs           # Database schema
│   │   └── queries.rs          # SQL queries
│   ├── handlers/
│   │   ├── execute.rs          # Handle execution requests
│   │   ├── status.rs           # Get execution status
│   │   └── kill.rs             # Terminate execution
│   └── error.rs                # Error types
├── Cargo.toml                  # Dependencies: tokio, sqlx, uuid, etc.
├── migrations/
│   └── 001_initial_schema.sql  # Create tables
├── Dockerfile                  # Container image
└── tests/
    ├── unit/
    │   └── executor_tests.rs   # Test execution logic
    └── integration/
        └── end_to_end_tests.rs # Test full flow
```

**What it does:**
1. Receives execution request
2. Creates execution record in database
3. Spins up isolated runtime (process/container/wasm)
4. Captures stdout/stderr in real-time
5. Monitors execution (CPU, memory, timeout)
6. Terminates when complete or timeout
7. Stores result in database
8. Streams logs back to client

**Why it's separate:**
- Can scale independently (multiple runtime instances)
- Can be deployed close to compute resources
- Easier to manage lifecycle separately

**Concepts you learn:**
- Process lifecycle management (spawn, monitor, kill)
- Async task spawning with Tokio
- Working with child processes (std::process::Command)
- Stream handling (async channels)
- Database interactions (sqlx, migrations)
- Error handling in long-running tasks
- Resource management (cleanup on errors)

### `packages/shared-types/`
**The bridge between frontend and backend.**

```
packages/shared-types/
├── src/
│   ├── lib.rs
│   ├── execution.rs            # ExecutionRequest, ExecutionResult
│   ├── workload.rs             # Workload definition
│   ├── error.rs                # Shared error types
│   └── api.rs                  # API contract
├── Cargo.toml                  # Used by services
└── typescript/                 # Generated TypeScript types
    └── index.ts                # For frontend type-safety
```

**What it does:**
- Defines data structures shared across services
- Ensures type safety across Rust and TypeScript
- Acts as the API contract

**Example:**
```rust
// In packages/shared-types/src/execution.rs
#[derive(Serialize, Deserialize, Clone)]
pub struct ExecutionRequest {
    pub id: String,
    pub language: String,      // "python", "javascript", "rust"
    pub code: String,
    pub timeout_ms: u64,       // How long to run
    pub environment: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub struct ExecutionResult {
    pub id: String,
    pub status: ExecutionStatus,  // Running, Completed, Failed, Timeout
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}
```

### `infrastructure/docker/`
**Where we define container images.**

#### `infrastructure/docker/runtime/`
**The image that actually executes workloads.**

```
infrastructure/docker/runtime/
├── Dockerfile
├── entrypoint.sh
└── requirements.txt            # Python packages pre-installed
```

**This Dockerfile is IMPORTANT:**

```dockerfile
FROM python:3.11-slim
FROM node:20-alpine
FROM rust:latest-bookworm

# Install execution environments
RUN apt-get update && apt-get install -y \
    python3 python3-pip \
    nodejs npm \
    && rm -rf /var/lib/apt/lists/*

# Copy entrypoint script
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
```

**Why this matters:**
- This is the container your user code runs inside
- It's isolated from the host system
- It has resource limits (memory, CPU)
- User code CANNOT access the host

**Concepts you learn:**
- Docker image construction
- Multi-language runtime environments
- Container isolation mechanisms
- Resource constraints in containers

#### `infrastructure/docker/services/`
**Dockerfiles for our own services.**

Each service gets its own Dockerfile showing how to build it.

### `infrastructure/kubernetes/`
**Empty until Phase 4.** Don't touch yet.

### `scripts/dev/`
**Local development automation.**

```
scripts/dev/setup.sh           # Downloads dependencies, sets up databases
scripts/dev/run-local.sh       # Starts all services locally
```

### `docs/`
**Learning materials alongside code.**

- `CONCEPTS.md` - Explains async Rust, Docker, Linux concepts
- `LEARNING_PATH.md` - "Read these docs before Phase X"
- `ARCHITECTURE.md` - How services communicate
- `API.md` - API endpoint documentation

---

# 🔄 How Services Communicate

## Phase 1: Simple HTTP + WebSocket

```
┌──────────────────┐
│  Next.js Client  │
└────────┬─────────┘
         │
         │ HTTP (JSON)
         ▼
┌──────────────────────────┐
│   API Gateway (Axum)     │
│  - Validates requests    │
│  - Routes to runtime     │
│  - Streams logs          │
└────────┬─────────────────┘
         │
         │ HTTP (gRPC-like)
         ▼
┌──────────────────────────┐
│  Runtime Service (Axum)  │
│  - Executes workload     │
│  - Manages lifecycle     │
│  - Stores in database    │
└──────────────────────────┘
         │
         ▼
    ┌────────────┐
    │ PostgreSQL │
    │ SQLite (dev)
    └────────────┘
```

## Phase 3: Queues + Scheduling

```
┌──────────────────┐
│   API Gateway    │────────┐
└──────────────────┘        │
                            ▼
                   ┌────────────────┐
                   │ Execution Queue │ (NATS/Kafka)
                   │  (persistent)   │
                   └────────┬────────┘
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
    ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
    │ Runtime 1   │  │ Runtime 2   │  │ Runtime 3   │
    │ (Worker)    │  │ (Worker)    │  │ (Worker)    │
    └─────────────┘  └─────────────┘  └─────────────┘
```

This is what enables **horizontal scaling**.

---

# 🧠 What Each Service Teaches You

## API Gateway Learning Outcomes
- ✅ Axum web framework
- ✅ Request/response handling
- ✅ Error propagation
- ✅ HTTP clients
- ✅ WebSocket basics
- ✅ Input validation
- ✅ Middleware patterns

## Runtime Service Learning Outcomes
- ✅ Process spawning and management
- ✅ Child process communication
- ✅ Async task coordination
- ✅ Stream handling
- ✅ Database interactions
- ✅ Resource cleanup
- ✅ Timeout handling
- ✅ Error recovery

## Shared Types Learning Outcomes
- ✅ Cargo workspaces
- ✅ Type design for distributed systems
- ✅ Serialization/deserialization
- ✅ API contracts
- ✅ Cross-language type generation

---

# 🔀 The Execution Flow (End-to-End)

### User Sends Request

```json
{
  "language": "python",
  "code": "print('Hello from edge')\nprint(42)",
  "timeout_ms": 5000
}
```

### Step 1: API Gateway Receives Request
```rust
// api-gateway/src/handlers/execute.rs
POST /execute

1. Parse JSON into ExecutionRequest
2. Validate: language is supported, code not empty, timeout reasonable
3. Generate UUID: execution_id = "exec_abc123xyz"
4. Call runtime service: POST http://runtime-service:8081/execute
   - Pass execution_id + request
5. Return: { execution_id: "exec_abc123xyz" }
```

### Step 2: Runtime Service Starts Execution
```rust
// runtime-service/src/executor/process.rs

1. Create database record:
   INSERT INTO executions (
     id, language, code, status, created_at
   ) VALUES (...)

2. Spawn child process:
   Command::new("python")
     .arg("-c")
     .arg(&code)
     .stdout(Stdio::piped())
     .stderr(Stdio::piped())
     .spawn()

3. Get handles to stdout/stderr

4. Spawn async task to read output:
   - Read stdout line by line
   - Store in temporary buffer
   - Stream to WebSocket if connected
```

### Step 3: Client Receives Execution ID
```typescript
// dashboard-web/app/execute-workload.tsx

const response = await fetch('/api/execute', {
  method: 'POST',
  body: JSON.stringify({...})
});

const { execution_id } = await response.json();

// Now connect to WebSocket
const ws = new WebSocket(`/ws/logs/${execution_id}`);
```

### Step 4: Client Connects to Logs Stream
```rust
// api-gateway/src/handlers/logs.rs
GET /ws/logs/{execution_id}

1. Upgrade HTTP to WebSocket
2. Call runtime service: GET /logs/{execution_id}
3. Stream from runtime service → client in real-time
```

### Step 5: Runtime Monitors Execution
```rust
// runtime-service/src/lifecycle/monitoring.rs

Loop every 100ms:
- Check if process still alive
- Read available stdout/stderr
- Store in database
- Send to WebSocket clients
- Check timeout (5000ms elapsed?)
  - If yes: terminate process
```

### Step 6: Process Completes or Times Out
```rust
Process exits with status code

1. Read final stdout/stderr
2. Record end_time
3. Update database:
   UPDATE executions
   SET status = 'COMPLETED',
       exit_code = 0,
       duration_ms = 234
   WHERE id = 'exec_abc123xyz'

4. Close WebSocket connection
```

### Step 7: Client Receives Final Result
```json
{
  "id": "exec_abc123xyz",
  "status": "COMPLETED",
  "stdout": "Hello from edge\n42",
  "stderr": "",
  "exit_code": 0,
  "duration_ms": 234
}
```

### The Complete Timeline
```
t=0ms    | Client sends request
t=5ms    | API Gateway receives, generates ID
t=10ms   | Runtime service starts process
t=15ms   | First output arrives: "Hello from edge"
t=20ms   | Output: "42"
t=234ms  | Process exits
t=240ms  | Client receives final result + closes WebSocket
```

---

# 🛠️ Rust Project Structure Pattern

Every Rust service follows this pattern:

```
services/SERVICE_NAME/
├── src/
│   ├── main.rs                    # Entry point + server setup
│   │
│   ├── handlers/                  # HTTP handlers
│   │   ├── mod.rs                 # Re-exports
│   │   ├── execute.rs             # Handler for /execute
│   │   ├── status.rs              # Handler for /status
│   │   └── logs.rs                # Handler for /logs
│   │
│   ├── services/                  # Business logic (separate from HTTP)
│   │   ├── mod.rs
│   │   ├── executor.rs            # Actual execution logic
│   │   ├── lifecycle.rs           # Start/stop/monitor
│   │   └── logger.rs              # Log capture/storage
│   │
│   ├── models/                    # Data structures
│   │   ├── mod.rs
│   │   ├── execution.rs           # Execution model
│   │   └── workload.rs            # Workload model
│   │
│   ├── db/                        # Database layer
│   │   ├── mod.rs
│   │   ├── connection.rs          # Connection pool
│   │   └── queries.rs             # SQL queries as functions
│   │
│   ├── error.rs                   # Error types (impl Error)
│   ├── config.rs                  # Configuration from env vars
│   └── lib.rs                     # Optional: re-export public APIs
│
├── tests/
│   ├── unit/                      # Unit tests (small, focused)
│   │   └── executor_tests.rs
│   └── integration/               # Integration tests (full flow)
│       └── e2e_tests.rs
│
├── Cargo.toml                     # Dependencies + metadata
├── Cargo.lock                     # Locked versions
├── migrations/                    # SQL schema migrations
│   ├── 001_initial.sql
│   └── 002_add_timeout_tracking.sql
├── Dockerfile                     # How to containerize
└── README.md                      # How to run this service
```

### Key Principle: Separation of Concerns

```rust
// ❌ DON'T DO THIS (tight coupling)
async fn handle_execute() {
    // HTTP parsing
    // validation
    // database query
    // process spawning
    // log reading
    // response building
    // ALL IN ONE FUNCTION
}

// ✅ DO THIS (clean separation)
// handlers/execute.rs - HTTP layer only
async fn handle_execute(req: ExecutionRequest) -> Result<ExecutionId> {
    let exec_id = services::executor::execute(&req).await?;
    Ok(exec_id)
}

// services/executor.rs - Business logic only
async fn execute(req: &ExecutionRequest) -> Result<ExecutionId> {
    let exec_id = uuid::Uuid::new_v4();
    db::create_execution(&exec_id, req).await?;
    
    let process = spawn_process(req).await?;
    monitor_process(process, &exec_id).await?;
    
    Ok(exec_id)
}

// services/lifecycle.rs - Process management only
async fn spawn_process(req: &ExecutionRequest) -> Result<Child> {
    // ONLY handles process::Command
}
```

---

# 📚 Concepts You MUST Understand Before Phase 1

## Rust Concepts

1. **Ownership + Borrowing**
   - Why Rust won't let you clone everything
   - Arc<Mutex<T>> for shared state

2. **Traits**
   - How Axum handlers work with trait objects
   - Error trait implementation

3. **Lifetimes**
   - Why `'a` appears in signatures
   - Borrowed references in async functions

4. **async/await**
   - How .await works under the hood
   - Spawning tasks with tokio::spawn

5. **Channels**
   - tokio::sync::mpsc for communicating between tasks
   - How logs flow from process → client

6. **Error Handling**
   - Result<T, E> type
   - Propagation with ? operator
   - Custom error types with thiserror

## Docker Concepts

1. **Images vs Containers**
   - Image is the blueprint
   - Container is the instance

2. **Layers**
   - Each RUN/COPY creates a layer
   - Layers are cached

3. **Isolation**
   - Containers have separate filesystems
   - Network namespace
   - Process namespace

4. **Resource Limits**
   - Memory limits
   - CPU limits
   - Why this matters for security

## Linux Concepts

1. **Processes**
   - PID (process ID)
   - Parent/child relationships
   - Process exit codes

2. **Signals**
   - SIGTERM (graceful shutdown)
   - SIGKILL (force kill)
   - SIGPIPE (broken pipe)

3. **File Descriptors**
   - stdin (0), stdout (1), stderr (2)
   - Pipes and redirection
   - Non-blocking I/O

4. **Namespaces**
   - PID namespace (process isolation)
   - Network namespace (network isolation)
   - Filesystem namespace (filesystem isolation)

5. **cgroups**
   - Limit CPU for a process group
   - Limit memory for a process group
   - How containers use cgroups

---

# 🎓 Learning Path by Phase

## Phase 0: Foundation (Weeks 1-2)

Before writing any code, study:

### Week 1
- [ ] Read: "Async Rust" (Tokio book chapters 1-3)
- [ ] Read: "Ownership and Borrowing" (Rust book chapters 4-5)
- [ ] Read: "Traits" (Rust book chapter 10)
- [ ] Practice: Build a simple Tokio echo server
- [ ] Practice: Build a simple HTTP server with Axum

### Week 2
- [ ] Read: "Docker in Action" (chapters 1-4)
- [ ] Practice: Build and run Docker images
- [ ] Read: "Linux Process Management" (basics)
- [ ] Practice: Use strace to understand system calls
- [ ] Watch: "How containers work" (YouTube) x3

## Phase 1: Local Runtime (Weeks 3-4)

With foundation solid:

### Week 3
- [ ] Design: Draw execution flow on whiteboard
- [ ] Create: Monorepo structure
- [ ] Create: packages/shared-types
- [ ] Create: services/api-gateway skeleton
- [ ] Study: Axum request/response patterns

### Week 4
- [ ] Create: services/runtime-service skeleton
- [ ] Implement: Process spawning (std::process::Command)
- [ ] Implement: Stdout/stderr capturing
- [ ] Implement: Database schema
- [ ] Test: End-to-end local execution

## Phase 2: Containerization (Weeks 5-6)

Build on Phase 1:

### Week 5
- [ ] Learn: Docker container lifecycle
- [ ] Create: Dockerfile for runtime
- [ ] Implement: Docker integration in runtime-service
- [ ] Test: Execute code inside containers

### Week 6
- [ ] Add: Resource limits to containers
- [ ] Add: Network isolation
- [ ] Test: Multiple workloads in parallel
- [ ] Benchmark: Performance comparison

---

# ⚠️ What NOT to Do

- ❌ Build fancy UI before runtime works
- ❌ Add authentication before core engine works
- ❌ Deploy to AWS before local testing works
- ❌ Use message queues before single-node works
- ❌ Add Kubernetes before understanding the runtime
- ❌ Build AI agents before execution is solid
- ❌ Optimize prematurely
- ❌ Add "features" you haven't learned yet

---

# 📝 Next Steps

1. **You** study Phases 0 concepts (async Rust, Docker, Linux)
2. **I** create monorepo folder structure
3. **I** create Cargo.toml workspace files
4. **You** understand each service's responsibility
5. **I** create service skeletons with comments
6. **You** understand the execution flow completely
7. **Then** we start Phase 1 implementation together

---

# 🤔 Questions for You

Before we proceed:

1. **Rust knowledge**: Have you written async Rust before?
2. **Docker knowledge**: How comfortable are you with containers?
3. **Linux knowledge**: Familiar with processes, signals, file descriptors?
4. **Learning pace**: Want to learn concepts deeply or move faster?
5. **Time commitment**: How much time per week for this project?

These answers will help me adjust the learning path.
