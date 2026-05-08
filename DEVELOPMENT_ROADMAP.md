# Development Roadmap - Exact Steps

## Summary: What We're Building (The 5-Step Journey)

```
PHASE 0: Foundation
├─ Learn async Rust fundamentals
├─ Understand Docker basics
├─ Learn Linux process concepts
└─ Result: Knowledge, not code yet

PHASE 1: Single-Machine Runtime ⭐ START HERE
├─ Build API Gateway (Axum server)
├─ Build Runtime Service (process executor)
├─ Create PostgreSQL schema
├─ Test: Execute Python/JS/Shell code locally
└─ Result: Can execute ANY code safely

PHASE 2: Container Isolation
├─ Add Docker execution
├─ Set resource limits
├─ Add security constraints
└─ Result: Sandboxed execution

PHASE 3: Distributed System
├─ Add task queues (NATS/Kafka)
├─ Add scheduler
├─ Add multiple worker nodes
└─ Result: Horizontal scaling

PHASE 4: Kubernetes
├─ Add K8s manifests
├─ Deploy as micro-services
├─ Add operators
└─ Result: Cloud-native deployment

PHASE 5: AI Runtime
├─ Add agent execution layer
├─ Add memory persistence
├─ Add workflow orchestration
└─ Result: AI-first platform
```

---

## Phase 0: Foundation Knowledge (NOT Coding)

### What You Need to Learn

**Rust Async & Tokio**
- [ ] Read: The Tokio Tutorial (https://tokio.rs/tokio/tutorial)
- [ ] Focus: async/await, spawning tasks, channels
- [ ] Practice: Write a simple async server that echoes messages

**Rust Traits & Error Handling**
- [ ] Read: Rust Book Chapter 10 (Traits)
- [ ] Read: Rust Book Chapter 9 (Error Handling)
- [ ] Practice: Implement custom error types with thiserror crate

**Docker Fundamentals**
- [ ] Read: Docker in Action (Chapter 1-4)
- [ ] Practice: Build, run, and push images
- [ ] Understand: Layers, volumes, networking

**Linux Process Management**
- [ ] Learn: Process lifecycle (fork, exec, exit)
- [ ] Learn: Signals (SIGTERM, SIGKILL)
- [ ] Learn: File descriptors (stdin, stdout, stderr)
- [ ] Practice: Use strace to trace system calls

**Key Commands to Practice**
```bash
# Process inspection
ps aux | grep python
kill -TERM <pid>
kill -9 <pid>
strace -e trace=process python script.py

# Docker
docker build -t myimage:latest .
docker run -it --rm myimage:latest
docker ps -a
docker logs <container_id>

# File descriptors
ls -la /proc/<pid>/fd/
```

### Completion Criteria for Phase 0
- [ ] Can write async Rust code that compiles
- [ ] Can explain: ownership, borrowing, Arc<Mutex<T>>
- [ ] Can build and run Docker images
- [ ] Understand how process spawning works
- [ ] Understand stdout/stderr redirection

**Timeline: 2 weeks max**

---

## Phase 1: Local Single-Machine Runtime (The Core)

### Goal: Execute Arbitrary Code Locally

**What the user can do after Phase 1:**
```json
POST http://localhost:8080/execute
{
  "language": "python",
  "code": "print('Hello')"
}

// Response
{
  "execution_id": "exec_123abc",
  "status": "RUNNING"
}

// WebSocket: ws://localhost:8080/logs/exec_123abc
// Receives:
{
  "type": "stdout",
  "data": "Hello\n"
}

{
  "type": "completed",
  "exit_code": 0,
  "total_duration_ms": 45
}
```

### Deliverables Phase 1

#### 1. Monorepo Setup
```bash
ai-edge-runtime/
├── Cargo.toml                          # Workspace root
├── services/
│   ├── api-gateway/
│   │   ├── Cargo.toml
│   │   └── src/
│   └── runtime-service/
│       ├── Cargo.toml
│       └── src/
├── packages/
│   └── shared-types/
│       ├── Cargo.toml
│       └── src/
└── apps/
    └── dashboard-web/
        ├── package.json
        ├── src/
        └── app/
```

**Root Cargo.toml:**
```toml
[workspace]
members = [
    "services/api-gateway",
    "services/runtime-service",
    "packages/shared-types",
]
resolver = "2"
```

#### 2. Shared Types Package
**File: `packages/shared-types/src/lib.rs`**

```rust
// Data structures used by all services
pub struct ExecutionRequest {
    pub id: String,
    pub language: String,      // "python", "javascript", "bash", "ruby"
    pub code: String,
    pub timeout_ms: u64,
    pub env_vars: HashMap<String, String>,
}

pub struct ExecutionResult {
    pub id: String,
    pub status: ExecutionStatus,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Timeout,
}
```

#### 3. API Gateway Service

**Responsibility:**
- Expose HTTP API
- Validate requests
- Forward to runtime service
- Stream logs back to client

**Key Files:**
- `services/api-gateway/src/main.rs` - Server setup
- `services/api-gateway/src/handlers/execute.rs` - POST /execute
- `services/api-gateway/src/handlers/logs.rs` - WebSocket /logs/{id}
- `services/api-gateway/src/client/runtime.rs` - HTTP client to runtime

**Endpoints:**
```
POST   /execute                    # Submit execution
GET    /execution/:id              # Get status
GET    /logs/:id                   # WebSocket stream
DELETE /execution/:id              # Kill execution
```

#### 4. Runtime Service

**Responsibility:**
- Execute workloads
- Manage process lifecycle
- Capture and stream logs
- Store results in database

**Key Files:**
- `services/runtime-service/src/main.rs` - Server setup
- `services/runtime-service/src/executor/process.rs` - Process spawning
- `services/runtime-service/src/lifecycle/monitor.rs` - Monitor execution
- `services/runtime-service/src/logs/stream.rs` - Log streaming
- `services/runtime-service/src/db/mod.rs` - Database layer

**Database Schema:**
```sql
CREATE TABLE executions (
    id VARCHAR(36) PRIMARY KEY,
    language VARCHAR(20),
    code TEXT,
    status VARCHAR(20),
    stdout TEXT,
    stderr TEXT,
    exit_code INT,
    duration_ms BIGINT,
    created_at TIMESTAMP,
    completed_at TIMESTAMP
);

CREATE TABLE log_entries (
    id BIGSERIAL PRIMARY KEY,
    execution_id VARCHAR(36),
    stream_type VARCHAR(10),  -- 'stdout' or 'stderr'
    data TEXT,
    timestamp TIMESTAMP,
    FOREIGN KEY (execution_id) REFERENCES executions(id)
);
```

#### 5. Docker Compose for Local Development

**File: `docker-compose.yml`**
```yaml
version: '3.8'
services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_DB: execution_db
      POSTGRES_USER: dev
      POSTGRES_PASSWORD: dev
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data

volumes:
  postgres_data:
```

#### 6. Integration Tests

**File: `services/api-gateway/tests/e2e.rs`**
```rust
#[tokio::test]
async fn test_execute_python_code() {
    // 1. Start both services
    // 2. Send POST /execute with Python code
    // 3. Connect WebSocket to /logs/{id}
    // 4. Receive log events
    // 5. Verify output
    // 6. Verify database record
}
```

### Phase 1 Execution Flow

```
┌─────────────────────────────────────────────────────────┐
│ User sends: POST /execute {"language":"python", ...}   │
└──────────────────┬──────────────────────────────────────┘
                   │
        ┌──────────▼─────────┐
        │  API Gateway       │
        │ (Axum on port 8080)│
        │                    │
        │ 1. Parse JSON      │
        │ 2. Validate input  │
        │ 3. Generate ID     │
        │ 4. Call runtime    │
        └──────────┬─────────┘
                   │ HTTP request
        ┌──────────▼──────────────┐
        │  Runtime Service        │
        │ (Axum on port 8081)     │
        │                         │
        │ 1. Save to DB           │
        │ 2. Spawn process        │
        │    Command::new("python")
        │    .arg("-c")           │
        │    .arg(&code)          │
        │    .stdout(Stdio::piped)│
        │    .stderr(Stdio::piped)│
        │    .spawn()             │
        │ 3. Read outputs         │
        │ 4. Update DB            │
        └──────────┬──────────────┘
                   │
        ┌──────────▼─────────────┐
        │  PostgreSQL            │
        │  Store execution       │
        │  Store logs            │
        └────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│ Client connects: WebSocket /logs/{id}                  │
│ Receives: { type: "stdout", data: "output..." }        │
│ Then: { type: "completed", exit_code: 0 }             │
└─────────────────────────────────────────────────────────┘
```

### What You Learn in Phase 1

**Rust & Tokio:**
- Building HTTP servers with Axum
- Handling async request/response
- Spawning background tasks
- Using channels for communication
- Error handling in async code
- Database access with sqlx

**Systems Programming:**
- Process spawning with std::process::Command
- Stdout/stderr redirection
- Waiting for process completion
- Exit code handling
- Resource cleanup

**Architecture:**
- Service separation (API vs execution)
- Request/response patterns
- Database schema design
- Streaming protocols (WebSocket basics)

### Testing Strategy Phase 1

```rust
#[cfg(test)]
mod tests {
    // Unit tests - test individual functions
    #[test]
    fn test_validate_execution_request() { }
    
    #[test]
    fn test_spawn_process_with_valid_code() { }
    
    // Integration tests - test full flow
    #[tokio::test]
    async fn test_execute_python_returns_output() { }
    
    #[tokio::test]
    async fn test_timeout_kills_long_running_process() { }
    
    #[tokio::test]
    async fn test_stderr_captured_separately() { }
}
```

### Phase 1 Success Criteria

- [ ] Can execute Python code: `print('hello')`
- [ ] Can execute JavaScript code: `console.log('hello')`
- [ ] Can execute Bash code: `echo hello`
- [ ] Stdout captured correctly
- [ ] Stderr captured correctly
- [ ] Timeout kills process after N milliseconds
- [ ] Exit codes preserved
- [ ] WebSocket streaming works
- [ ] Database records created
- [ ] 10+ integration tests pass
- [ ] Local execution feels smooth and fast

**Timeline: 2-3 weeks**

---

## Phase 2: Container Isolation (Brief Overview)

After Phase 1 works, add containers:

```
// Phase 1
Command::new("python").spawn()

// Phase 2
Docker::run_container("python:3.11", code).await

// Benefits
- Isolation from host
- Resource limits
- Security boundaries
- Easy multi-language support
```

---

## The Key Question You Should Ask Yourself

**"Why does the runtime execute code OUTSIDE containers in Phase 1?"**

Answer: Because you need to learn the fundamentals first.
- Understand process management
- Understand I/O handling
- Understand lifecycle control
- THEN add the complexity of containers

This is how real platforms are built.

---

## Success Metric for Understanding This Plan

Can you answer these questions?

1. **"What does the API Gateway do?"**
   - (Should be: "Receives requests, validates, forwards to runtime, streams logs")

2. **"What does the Runtime Service do?"**
   - (Should be: "Spawns processes, captures I/O, manages lifecycle, stores results")

3. **"Why are they separate services?"**
   - (Should be: "Separation of concerns, independent scaling, clear contracts")

4. **"How do logs stream in real-time?"**
   - (Should be: "Process stdout → captured → WebSocket → client")

5. **"What is the database used for?"**
   - (Should be: "Storing execution metadata, logs, results for persistence")

If you can explain these clearly, you understand the architecture.

---

## Questions Before We Build

1. **Rust experience?** (Beginner/Intermediate/Advanced)
2. **Ever used Tokio?** (Yes/No)
3. **Docker experience?** (Beginner/Intermediate/Advanced)
4. **PostgreSQL experience?** (Never/Basic/Good)
5. **Time available per week?** (hours)
6. **Learning style?** (Deep dives on concepts / implement then learn)

These will help me calibrate the explanations and code comments.
