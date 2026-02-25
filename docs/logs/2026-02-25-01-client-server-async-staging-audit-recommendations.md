# Client/Server Async Staging Audit Recommendations

Date: 2026-02-25
Scope: CLI -> daemon API -> SQLite staging -> job worker execution, with focus on async provider boundaries, transactional staging, idempotency, and client-visible behavior.

## Files Reviewed

- `src/main.rs`
- `src/client.rs`
- `src/daemon/mod.rs`
- `src/daemon/routes.rs`
- `src/daemon/jobs.rs`
- `src/daemon/events.rs`
- `src/db/mod.rs`
- `migrations/0001_init.sql`
- `migrations/0002_environment_failed_status.sql`
- `migrations/0003_task_environment_not_null.sql`

## Executive Summary

The current design partially matches the intended model:

- `task new` returns `202` quickly and stages task+env records before provider work.
- provider work for task execution is in jobs (`prepare_environment`, `run_task`, `remove_environment`).

Major gaps remain relative to the target architecture:

1. API staging writes are not atomic in a single DB transaction.
2. Several provider-touching endpoints are still synchronous in request handling.
3. Task-bound env lifecycle still uses a `pool -> in_use` transition that permits race/steal behavior.
4. Job queue semantics are not strongly idempotent (no dedupe keys/leases/attempt policy).
5. Delete/cancel flows are not fully staged and can leave cross-entity inconsistencies.

## Current Behavior Matrix (Client -> Server)

### Project commands

- `project new/remove/list`: synchronous SQLite operations.
- This matches desired boundary (no provider involved).

### Environment commands

- `environment prepare`: async/staged (`202` + `prepare_environment` job).
- `environment remove`: async/staged (`202` + `remove_environment` job).
- `environment update`: sync provider execution in route.
- `environment claim`: sync provider execution in route.
- `environment claim-next`: sync provider execution in route.

Recommendation: all provider-backed environment operations should enqueue jobs and return `202`.

### Task commands

- `task new`: stages env + task records and enqueues `prepare_environment`; returns `202`.
- `prepare_environment` (when task-bound) enqueues `run_task` after prepare success.
- `run_task` performs provider claim + provider run + process spawn in worker.
- `task remove`: queues env removal but also deletes task synchronously in route.

Recommendation: model task removal/cancel as staged async orchestration with task state machine (not immediate hard delete in request path).

## Detailed Findings

### 1) Transactional staging is incomplete

`create_task` route performs 3 separate DB operations without one shared transaction:

1. create env
2. create task
3. create job

A failure in step 3 leaves created task+env with no queued work.

Recommendation:

- Add DB-level staged primitives using one transaction per API command, e.g.:
  - `db::stage_task_creation(...) -> {task, env, job}`
  - `db::stage_environment_prepare(...)`
  - `db::stage_task_cancel(...)`
- Route handlers should call one staging primitive and return.

### 2) Provider execution still blocks request path in multiple routes

The following routes call provider methods inline:

- `update_environment` (`provider.update`)
- `claim_environment` (`provider.claim`)
- `claim_next_environment` (`provider.claim`)

These can take arbitrarily long and violate async boundary expectations.

Recommendation:

- Convert to enqueue-only routes with `202` and stage rows first.
- Add job types:
  - `update_environment`
  - `claim_environment`
  - `claim_next_environment` (or collapse into `claim_environment` with resolver stage)

### 3) Task-bound environment can be claimed by unrelated flows

Current task path:

- task env moves to `pool` after prepare
- later `run_task` claims it to `in_use`

Between these stages, generic claim endpoints can grab it.

Recommendation:

- Split env statuses into pool-bound vs task-bound:
  - `preparing_task`, `ready_task`, `in_use`, `failed`
  - `preparing_pool`, `pool`, `in_use`, `failed`
- Enforce claim queries only on eligible status classes.
- For task-bound envs, skip `pool` entirely.

### 4) Job queue lacks robust idempotency model

Current jobs table has `id,type,payload,status` but no:

- dedupe key
- lease timeout/owner
- attempt count
- backoff policy

`claim_pending_jobs` claims all pending jobs each poll, then spawns all concurrently.

Recommendations:

- Add columns:
  - `dedupe_key TEXT UNIQUE NULL`
  - `attempt INTEGER NOT NULL DEFAULT 0`
  - `not_before TEXT NULL`
  - `lease_expires_at TEXT NULL`
  - `last_error TEXT NULL`
- Claim jobs with `LIMIT N` and lease semantics.
- Retry with bounded attempts and exponential backoff.
- Ensure all handlers are idempotent by state guards and monotonic transitions.

### 5) Task state machine is too coarse

Current task states: `pending|started|complete|failed`.
This obscures operational stage and weakens recovery clarity.

Recommendation:

- Expand states, e.g.:
  - `pending`
  - `env_preparing`
  - `env_ready`
  - `running`
  - `complete|failed|canceled`
- Transition only in worker stage handlers.
- Keep state transitions monotonic and validated in SQL `WHERE` clauses.

### 6) Delete/cancel behavior is not fully staged

`remove_task` currently:

- marks env removing
- enqueues remove env job
- deletes task immediately

If provider remove fails, env may remain failed/removing while task record is gone, reducing debuggability.

Recommendation:

- Replace hard delete with cancel/archive workflow:
  - stage `cancel_task` job (`202`)
  - worker updates task terminal state and drives env cleanup
  - optional GC job hard-deletes old terminal tasks later

### 7) API consistency is mixed (sync vs async semantics)

Some mutating endpoints return immediate final state (`200`), others return queued state (`202`).

Recommendation:

- Define policy:
  - provider-free mutations: `200/201/204`
  - provider-involved mutations: `202` + staged entity snapshot
- Add operation resource (optional): `/operations/{id}` to track queued operation status.

### 8) SSE update model is coarse

SSE only emits generic `update`, forcing full client polling.

Recommendation:

- Keep generic mode for now, but consider structured events:
  - `task.updated`, `environment.updated`, `job.updated`
- Allow clients to reduce full-list polling in high-load setups.

## Recommended Target Architecture

### Core principle

- Request handlers perform only validation + transactional staging.
- Workers perform all provider and long-running work.

### Suggested staged workflows

#### `POST /tasks` (`task new`)

In one transaction:

1. create env row in task-bound pending state
2. create task row in `pending`/`env_preparing`
3. enqueue `prepare_task` job with `{task_id, env_id}` and dedupe key
4. return `202` with task+env ids

Worker `prepare_task` (idempotent):

1. no-op if terminal
2. provider.prepare
3. mark env ready/in-use according to policy
4. enqueue `run_task` (or continue inline in same job if preferred)
5. on error: mark env/task failed

#### `POST /environments/{id}/claim`

In one transaction:

1. validate env is pool-claimable
2. enqueue `claim_environment`
3. return `202`

Worker claims with state guard and provider.claim.

#### `DELETE /tasks/{id}`

In one transaction:

1. mark task as `cancel_requested`
2. enqueue `cancel_task`
3. return `202`

Worker resolves process cancellation/cleanup and env removal idempotently.

## Prioritized Implementation Plan

### Phase 1 (safety + consistency)

1. Add transactional staging helpers in `db` for task/env/job triple writes.
2. Convert provider-touching routes (`claim/update/claim-next`) to enqueue-only `202`.
3. Introduce task-bound env statuses to eliminate pool-steal race.

### Phase 2 (idempotent queue semantics)

1. Add queue metadata (attempt, not_before, lease, dedupe_key).
2. Change job claim loop to `LIMIT N` + lease ownership.
3. Add retry/backoff and max-attempt terminal failure policy.

### Phase 3 (lifecycle/observability)

1. Expand task/env state machines.
2. Replace hard delete with cancel/archive + deferred GC.
3. Add structured SSE events or operation status resources.

## Client/CLI Recommendations

- Keep CLI blocking only for user-intent convenience flags (`--attach` logs), not for backend mutation completion.
- Default mutating commands that involve provider to print staged IDs/status and return immediately.
- For human output, clearly show `queued`, `preparing`, `running`, `failed` transitions.

## Test Coverage Gaps to Add

1. Transactional atomicity tests for all staged route writes.
2. Concurrency/race tests for task-bound env vs pool claims.
3. Idempotent reprocessing tests for each job type.
4. Crash-recovery tests between each stage boundary.
5. End-to-end tests for cancel/remove semantics with provider failures.

## Conclusion

The codebase is close to the desired direction for `task new`, but still mixed in execution model. The highest-value next step is to finish the async boundary rule consistently: provider involvement should happen only in worker jobs, and request-side staging should be done in a single DB transaction.
