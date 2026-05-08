# 🎯 Executive Summary - The Plan

## What We're Building

A **distributed serverless execution platform** that can safely execute arbitrary user code at scale.

**Phase 1 Goal (2-3 weeks):** Execute Python/JavaScript/Bash code locally with streaming logs.

---

## The Three Core Services (Phase 1)

### 1. **API Gateway** (Port 8080)
- Receives: `POST /execute { language, code, timeout }`
- Returns: `{ execution_id }`
- Streams: Logs via WebSocket `/ws/logs/:id`
- Tech: Rust + Axum framework

### 2. **Runtime Service** (Port 8081)
- Spawns processes: `Command::new("python")`
- Captures stdout/stderr in real-time
- Manages lifecycle (start, monitor, kill, timeout)
- Stores results in PostgreSQL
- Tech: Rust + Tokio + SQLx

### 3. **PostgreSQL Database**
- Stores execution metadata
- Stores logs
- Used for querying past executions

---

## The Request Flow (Simplified)

```
You (client)
    ↓
    POST /execute
        ↓
    API Gateway (validate, generate ID)
        ↓
        HTTP to Runtime Service
            ↓
            Spawn Process: python -c "your code"
                ↓
                Capture output
                    ↓
                    Save to DB
                        ↓
                        Stream to client via WebSocket
                            ↓
                        You receive live logs + result
```

**Total time:** ~50-100ms from request to completion

---

## Monorepo Structure (After Phase 1)

```
ai-edge-runtime/
├── apps/
│   └── dashboard-web/           (Next.js frontend - minimal)
├── services/
│   ├── api-gateway/             (Rust, HTTP server)
│   └── runtime-service/         (Rust, execution engine)
├── packages/
│   └── shared-types/            (Rust, TypeScript types)
├── infrastructure/
│   ├── docker/                  (Container images)
│   └── kubernetes/              (Empty for Phase 4)
├── docs/
│   ├── ARCHITECTURE_PLAN.md     (Everything explained)
│   ├── DEVELOPMENT_ROADMAP.md   (Exact steps)
│   └── VISUAL_ARCHITECTURE.md   (Diagrams + flows)
└── docker-compose.yml           (PostgreSQL for dev)
```

---

## Phase 0: Learn Before Building

Before we code, you should know:

### Rust (1 week)
- [ ] async/await basics
- [ ] Tokio spawning tasks
- [ ] Ownership + borrowing
- [ ] Traits + error handling
- [ ] Read: https://tokio.rs/tokio/tutorial

### Docker (1 week)
- [ ] How images work
- [ ] Build and run containers
- [ ] Volumes and networking
- [ ] Resource limits

### Linux (1 week)
- [ ] Process lifecycle
- [ ] Signals (SIGTERM, SIGKILL)
- [ ] File descriptors (stdout, stderr)
- [ ] Command: `strace`, `ps`, `kill`

**Total Phase 0 time:** 2-3 weeks (YOU decide pace)

---

## Phase 1: Build the Engine (2-3 weeks)

### Week 1: Setup
- [ ] Create monorepo structure
- [ ] Create Cargo.toml workspace
- [ ] Create shared-types package
- [ ] Design database schema
- [ ] Set up PostgreSQL with docker-compose

### Week 2: API Gateway
- [ ] Create api-gateway service
- [ ] Implement: POST /execute
- [ ] Implement: GET /logs/:id (WebSocket)
- [ ] Call runtime-service
- [ ] Error handling

### Week 3: Runtime Service
- [ ] Create runtime-service
- [ ] Implement: POST /execute/:id
- [ ] Spawn processes with Python/JS/Bash
- [ ] Capture stdout/stderr
- [ ] Store in database
- [ ] Monitor + timeout handling

### Week 4: Integration + Testing
- [ ] Test end-to-end
- [ ] Write integration tests
- [ ] Handle errors gracefully
- [ ] Performance optimization

---

## Success Criteria for Phase 1

After Phase 1, you can do this:

```bash
# Terminal 1: Start services
cargo run --package api-gateway
cargo run --package runtime-service
docker-compose up postgres

# Terminal 2: Send code
curl -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "language": "python",
    "code": "print(\"Hello from edge\")\nprint(42)"
  }'

# Response
{
  "execution_id": "exec_abc123"
}

# Terminal 3: Connect to logs
websocat ws://localhost:8080/ws/logs/exec_abc123

# Receive
{"type": "stdout", "data": "Hello from edge\n"}
{"type": "stdout", "data": "42\n"}
{"type": "completed", "exit_code": 0, "duration_ms": 45}
```

---

## What You Learn

| Topic | By Doing |
|-------|----------|
| **Rust async** | Building Tokio server |
| **Web frameworks** | Using Axum for HTTP |
| **Process management** | Spawning Python/Node processes |
| **Systems I/O** | Reading stdout/stderr |
| **Databases** | SQL schema + sqlx queries |
| **Architecture** | Service separation + contracts |
| **Error handling** | Propagating errors gracefully |
| **Real-time communication** | WebSocket streaming |

---

## Key Insight

**This is NOT a web app.**

You're building **infrastructure**, not features.

The mindset:
- ❌ "How do I make the UI pretty?"
- ✅ "How do I execute code reliably?"

That's the difference between a developer and a platform engineer.

---

## Commitment

This project requires:

1. **Patience** - Don't skip Phase 0
2. **Depth** - Understand WHY, not just HOW
3. **Hands-on** - Type every line, read every error
4. **Time** - ~8-12 weeks to reach Phase 3

By the end:
- You understand distributed systems
- You can build cloud infrastructure
- You have a production-ready learning project
- You can explain your platform in interviews

---

## Next Step: Your Feedback

Before we start building, I need to know:

1. **Rust experience?**
   - Never written Rust
   - Written some Rust
   - Comfortable with Rust

2. **Async Rust?**
   - No experience
   - Seen it, not written it
   - Comfortable with async

3. **Docker?**
   - Never used Docker
   - Used Docker images
   - Built Docker images

4. **PostgreSQL?**
   - Never used SQL
   - Written basic SQL
   - Comfortable with SQL

5. **Time commitment?** (hours/week)
   - 5-10 hours
   - 10-20 hours
   - 20+ hours

6. **Learning style?**
   - Deep theory first, then code
   - Show code, explain after
   - Just get started

7. **Anything you want to prioritize or skip?**

---

## If You're Ready

Once you confirm understanding and answer the questions above, we'll:

1. **Create monorepo structure**
2. **Set up Cargo workspace**
3. **Create service skeletons with detailed comments**
4. **Write step-by-step guide for Phase 0 learning**
5. **Build Phase 1 together** (I'll code alongside you, explaining every decision)

---

## Documents to Read First

Before giving feedback:

1. **ARCHITECTURE_PLAN.md** - Full explanation of services (20 min read)
2. **VISUAL_ARCHITECTURE.md** - Diagrams and execution flow (15 min read)
3. **DEVELOPMENT_ROADMAP.md** - Exact steps per phase (10 min read)

Then answer the 7 questions above.

You're not jumping into coding yet. We're building mental models first.

**That's the senior engineer approach.**
