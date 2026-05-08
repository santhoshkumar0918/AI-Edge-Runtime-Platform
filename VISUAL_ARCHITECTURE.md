# Visual Architecture Guide

## Phase 1: Single-Machine Runtime Architecture

### High-Level View

```
┌─────────────────────────────────────────────────────────────────────┐
│                          USER (You)                                 │
│                   Using curl/Postman/Frontend                       │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           │ HTTP POST
                           ▼
        ┌──────────────────────────────────────┐
        │        API GATEWAY (Axum)            │
        │      Port: 8080 (HTTP)               │
        │      Port: 8080 (WebSocket)          │
        │                                      │
        │  POST /execute                       │
        │  ├─ Parse request                    │
        │  ├─ Validate code                    │
        │  ├─ Generate execution_id            │
        │  └─ Call runtime service             │
        │                                      │
        │  GET /ws/logs/:id                    │
        │  └─ Stream logs via WebSocket        │
        └──────────────────────────────────────┘
                           │
                           │ HTTP REST
                           │
        ┌──────────────────────────────────────┐
        │   RUNTIME SERVICE (Axum)             │
        │     Port: 8081 (HTTP)                │
        │                                      │
        │  POST /execute/:id                   │
        │  ├─ Insert into DB                   │
        │  ├─ Spawn process                    │
        │  │  Command::new("python")           │
        │  │    .arg("-c")                     │
        │  │    .arg(&code)                    │
        │  │    .stdout(piped)                 │
        │  │    .stderr(piped)                 │
        │  │    .spawn()                       │
        │  │                                   │
        │  ├─ Monitor process                  │
        │  │  - Check if running               │
        │  │  - Read stdout                    │
        │  │  - Read stderr                    │
        │  │  - Check timeout                  │
        │  │  - Kill if needed                 │
        │  │                                   │
        │  ├─ Capture output                   │
        │  │  └─ Store in DB                   │
        │  │                                   │
        │  └─ Return result                    │
        │                                      │
        │  GET /logs/:id                       │
        │  └─ Stream logs from storage         │
        └──────────────────────────────────────┘
                           │
                           │ SQL
                           ▼
        ┌──────────────────────────────────────┐
        │       POSTGRESQL DATABASE            │
        │                                      │
        │  executions:                         │
        │  ├─ id (UUID)                        │
        │  ├─ language                         │
        │  ├─ code                             │
        │  ├─ status                           │
        │  ├─ stdout                           │
        │  ├─ stderr                           │
        │  ├─ exit_code                        │
        │  ├─ duration_ms                      │
        │  └─ created_at                       │
        │                                      │
        │  log_entries:                        │
        │  ├─ execution_id (FK)                │
        │  ├─ stream_type (stdout/stderr)      │
        │  ├─ data                             │
        │  └─ timestamp                        │
        └──────────────────────────────────────┘
```

### Detailed Request Flow

```
STEP 1: Client Submits Code
───────────────────────────

Client POST /execute:
{
  "language": "python",
  "code": "print('Hello, World!')\nprint(42)"
  "timeout_ms": 5000
}

Response (immediate):
{
  "execution_id": "exec_550e8400-e29b-41d4-a716-446655440000"
  "status": "PENDING"
}

Time: 0ms


STEP 2: API Gateway Receives
─────────────────────────────

API Gateway Handler:
1. Parse JSON
2. Validate:
   - Is language supported? ✓
   - Is code not empty? ✓
   - Is timeout reasonable? ✓
3. Generate UUID: exec_550e8...
4. Call runtime-service

Request to Runtime:
POST http://localhost:8081/execute/exec_550e8...

Body:
{
  "language": "python",
  "code": "print('Hello, World!')\nprint(42)",
  "timeout_ms": 5000
}

Time: 1-2ms


STEP 3: Runtime Service Initializes
────────────────────────────────────

Runtime Handler:
1. Create database record:
   
   INSERT INTO executions (
     id, language, code, status, created_at
   ) VALUES (
     'exec_550e8...', 
     'python', 
     'print(...)',
     'RUNNING',
     NOW()
   );

2. Get connection to process output
3. Spawn child process:

   let mut child = Command::new("python")
     .arg("-c")
     .arg("print('Hello, World!')\nprint(42)")
     .stdout(Stdio::piped())
     .stderr(Stdio::piped())
     .spawn()?;

4. Get stdout/stderr handles:
   
   let stdout = child.stdout.take().unwrap();
   let stderr = child.stderr.take().unwrap();

5. Spawn monitoring task:
   
   tokio::spawn(monitor_execution(
     child_id, 
     stdout, 
     stderr
   ));

Time: 3-5ms


STEP 4: Client Connects for Logs
─────────────────────────────────

Client WebSocket:
GET ws://localhost:8080/ws/logs/exec_550e8...

API Gateway:
1. Upgrade HTTP to WebSocket
2. Call runtime-service for logs:
   GET http://localhost:8081/logs/exec_550e8...
3. Stream responses to client

Time: 6-10ms


STEP 5: Process Executes
────────────────────────

Child Process stdout:
┌─────────────────────────────────┐
│ Hello, World!                   │
│ 42                              │
│ <process exits with code 0>     │
└─────────────────────────────────┘

Time: 11-50ms (depending on code)


STEP 6: Runtime Monitors Output
───────────────────────────────

Monitoring Task (every 100ms):
1. Check if process still alive? 
   - YES (exit_code = 0, child exited)
2. Read available stdout:
   - "Hello, World!\n42\n"
3. Read available stderr:
   - ""
4. Update database:
   
   UPDATE executions
   SET stdout = 'Hello, World!\n42\n',
       stderr = '',
       exit_code = 0,
       status = 'COMPLETED',
       duration_ms = 45
   WHERE id = 'exec_550e8...';

5. Send to WebSocket client:
   
   {
     "type": "stdout",
     "data": "Hello, World!\n"
   }
   
   {
     "type": "stdout",
     "data": "42\n"
   }
   
   {
     "type": "completed",
     "exit_code": 0,
     "duration_ms": 45
   }

Time: 51-55ms


STEP 7: Client Receives Full Result
────────────────────────────────────

Browser receives WebSocket messages:
✓ "Hello, World!"
✓ "42"
✓ Execution completed in 45ms

Time: 56ms (total)


STEP 8: Execution Stored
────────────────────────

Database state:
executions table:
├─ id: 'exec_550e8...'
├─ language: 'python'
├─ code: 'print(...)'
├─ status: 'COMPLETED'
├─ stdout: 'Hello, World!\n42\n'
├─ stderr: ''
├─ exit_code: 0
├─ duration_ms: 45
└─ created_at: 2024-01-15 14:30:00

log_entries table (optional):
├─ execution_id: 'exec_550e8...'
├─ stream_type: 'stdout'
├─ data: 'Hello, World!\n'
├─ timestamp: 14:30:00.010
├─ execution_id: 'exec_550e8...'
├─ stream_type: 'stdout'
├─ data: '42\n'
├─ timestamp: 14:30:00.045
```

---

## Folder Structure After Phase 1 Complete

```
ai-edge-runtime/
│
├── ARCHITECTURE_PLAN.md              ← Detailed explanation (you are here)
├── DEVELOPMENT_ROADMAP.md            ← Step-by-step what to build
├── Cargo.toml                        ← Rust workspace configuration
├── docker-compose.yml                ← Local PostgreSQL
│
├── apps/
│   └── dashboard-web/                ← Next.js frontend (minimal for now)
│       ├── package.json
│       ├── app/
│       ├── public/
│       └── tsconfig.json
│
├── services/
│   │
│   ├── api-gateway/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs               ← Entry point, sets up Axum server
│   │   │   ├── handlers/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── execute.rs        ← POST /execute
│   │   │   │   └── logs.rs           ← WebSocket /logs/:id
│   │   │   ├── client/
│   │   │   │   └── runtime.rs        ← HTTP client to runtime-service
│   │   │   ├── error.rs              ← Error types
│   │   │   └── config.rs             ← Configuration from env
│   │   ├── Dockerfile
│   │   └── tests/
│   │       └── e2e.rs
│   │
│   └── runtime-service/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs               ← Entry point, sets up Axum server
│       │   ├── handlers/
│       │   │   ├── mod.rs
│       │   │   ├── execute.rs        ← Handle execution requests
│       │   │   ├── status.rs         ← Get execution status
│       │   │   └── logs.rs           ← Stream logs
│       │   ├── executor/
│       │   │   ├── mod.rs
│       │   │   ├── process.rs        ← Spawn processes
│       │   │   └── monitor.rs        ← Monitor execution
│       │   ├── logs/
│       │   │   ├── mod.rs
│       │   │   ├── capture.rs        ← Read stdout/stderr
│       │   │   └── stream.rs         ← Stream to client
│       │   ├── models/
│       │   │   ├── mod.rs
│       │   │   └── execution.rs      ← ExecutionRequest/Result
│       │   ├── db/
│       │   │   ├── mod.rs
│       │   │   ├── connection.rs     ← PostgreSQL pool
│       │   │   └── queries.rs        ← SQL functions
│       │   ├── error.rs
│       │   └── config.rs
│       ├── migrations/
│       │   └── 001_initial.sql       ← Create tables
│       ├── Dockerfile
│       └── tests/
│           ├── unit/
│           └── integration/
│
├── packages/
│   └── shared-types/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── execution.rs          ← ExecutionRequest, ExecutionResult
│           ├── workload.rs
│           └── error.rs
│
├── infrastructure/
│   ├── docker/
│   │   ├── runtime/                  ← Phase 2: Container for user code
│   │   │   ├── Dockerfile
│   │   │   └── entrypoint.sh
│   │   └── services/                 ← Dockerfiles for our services
│   │       ├── api-gateway.Dockerfile
│   │       └── runtime-service.Dockerfile
│   └── kubernetes/                   ← Phase 4: K8s manifests (empty)
│
├── scripts/
│   ├── dev/
│   │   └── setup.sh                  ← Install dependencies, setup DB
│   └── testing/
│       └── test-execution.sh         ← Test executing code
│
└── docs/
    ├── CONCEPTS.md                   ← Async Rust, Docker, Linux concepts
    ├── LEARNING_PATH.md              ← What to study per phase
    ├── API.md                        ← API endpoint documentation
    └── TROUBLESHOOTING.md            ← Common issues
```

---

## The Key Files You Need to Understand

### 1. Root Workspace - `Cargo.toml`
```toml
[workspace]
members = [
    "services/api-gateway",
    "services/runtime-service",
    "packages/shared-types",
]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
axum = "0.7"
serde = { version = "1", features = ["derive"] }
sqlx = { version = "0.7", features = ["postgres"] }
```

### 2. API Gateway - `services/api-gateway/src/main.rs`
```rust
use axum::{
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // 1. Create routes
    let app = Router::new()
        .route("/execute", post(handlers::execute::handle))
        .route("/logs/:id", get(handlers::logs::handle));
    
    // 2. Listen on 8080
    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .unwrap();
    
    // 3. Run server
    axum::serve(listener, app)
        .await
        .unwrap();
}
```

### 3. Runtime Service - `services/runtime-service/src/main.rs`
```rust
use axum::Router;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // Similar to API Gateway but on port 8081
}
```

### 4. Execution Model - `packages/shared-types/src/execution.rs`
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub language: String,     // "python", "javascript", "bash"
    pub code: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub execution_id: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}
```

---

## Why This Architecture?

### Separation of Concerns

| Layer | Responsibility | Can Change | Independently Scaled |
|-------|-----------------|-----------|---------------------|
| **API Gateway** | HTTP, validation, routing | Easily | Yes |
| **Runtime Service** | Execution, I/O, lifecycle | Easily | Yes |
| **Database** | Persistence | Schema carefully | Yes |

### If You Change One Layer

```
Example: Want to use gRPC instead of HTTP?

Current: HTTP Gateway ←→ HTTP Runtime
New:     gRPC Gateway ←→ gRPC Runtime

Changes:
- api-gateway/src/client/runtime.rs (HTTP to gRPC)
- runtime-service: add gRPC handler

Everything else stays the same!
```

### Benefits for Learning

1. **Isolation** - Change one service without breaking others
2. **Testing** - Test each service independently
3. **Clarity** - Each service has ONE job
4. **Scalability** - Each service can scale separately
5. **Real-world** - This mirrors production architectures

---

## Now You Should Understand

1. **Monorepo structure** - Why services are separate
2. **API Gateway's role** - Front door of the platform
3. **Runtime Service's role** - Actually execute code
4. **Data flow** - Request → API → Runtime → DB → Response
5. **Why PostgreSQL** - For persistence, querying results
6. **Why Tokio** - For async I/O, spawning processes
7. **Why WebSocket** - For real-time log streaming

---

## Next: Questions for You

Before we start building, answer these:

1. **Have you written async Rust code before?**
   - [ ] Never
   - [ ] Little bit
   - [ ] Comfortable

2. **Have you used Axum or similar web frameworks?**
   - [ ] Never
   - [ ] Once or twice
   - [ ] Multiple times

3. **Do you understand process spawning in Unix?**
   - [ ] Not really
   - [ ] Basic understanding
   - [ ] Very comfortable

4. **How much time per week?** (hours)
   - [ ] 5-10
   - [ ] 10-20
   - [ ] 20+

5. **Learning preference?**
   - [ ] Theory first, then code
   - [ ] Code examples, then explain
   - [ ] Just show me what to type

Your answers help me calibrate explanations and examples.
