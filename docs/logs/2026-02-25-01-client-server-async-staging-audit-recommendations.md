# Client/Server Async Staging Audit Recommendations

Date: 2026-02-25
Scope: CLI -> daemon API -> SQLite staging -> job worker execution, with focus on async provider boundaries, transactional staging, idempotency, and client-visible behavior.

## Reviewed Surface

- CLI command flow: `src/main.rs`
- client transport and request semantics: `src/client.rs`
- daemon routing and request handlers: `src/daemon/mod.rs`, `src/daemon/routes.rs`
- async worker execution model: `src/daemon/jobs.rs`
- database lifecycle and persistence primitives: `src/db/mod.rs`
- state constraints and migrations: `migrations/0001_init.sql`
- provider interfaces and execution model: `src/environment/mod.rs`, `src/environment/script.rs`, `src/environment/git_worktree.rs`
- current tests: `tests/daemon.rs`, `tests/database.rs`

## Re-Review Findings (Severity Ordered)

### Critical

1. **Job recovery is broken after daemon crash or hard stop**
- jobs are claimed by changing status `pending -> running`, but there is no lease/heartbeat/requeue mechanism.
- if daemon exits while jobs are `running`, they are stranded forever and never retried.
- evidence:
  - `claim_pending_jobs` only selects `status = 'pending'`: `src/db/mod.rs:411-430`
  - no startup repair/requeue path in daemon startup: `src/daemon/mod.rs:36-140`
  - no lease columns in jobs schema: `migrations/0001_init.sql:30-37`

2. **Provider work still runs synchronously in API handlers**
- request path still performs unbounded provider operations for:
  - env update (`provider.update`)
  - env claim (`provider.claim`)
  - env claim-next (`provider.claim`)
- this violates the async boundary policy and can stall API calls indefinitely.
- evidence: `src/daemon/routes.rs:182-253`

3. **Staging writes are not atomic for multi-entity operations**
- task create flow performs environment row insert, task row insert, and job enqueue across separate DB connections without one transaction.
- partial writes are possible if later steps fail (for example job enqueue fails after env/task rows exist).
- same issue exists in env prepare/remove and task remove staging paths.
- evidence:
  - task create route: `src/daemon/routes.rs:314-330`
  - env prepare route: `src/daemon/routes.rs:136-149`
  - env remove route: `src/daemon/routes.rs:272-286`
  - task remove route: `src/daemon/routes.rs:379-395`
  - db helpers open separate connections per call: `src/db/mod.rs:9-13`

4. **`work env create` is racy and frequently incorrect**
- CLI does `prepare_environment` immediately followed by `claim_environment`.
- prepare is async and returns `202` with `preparing` state, so immediate claim often fails because env is not yet `pool`.
- evidence:
  - CLI flow: `src/main.rs:629-631`
  - prepare is async queued: `src/daemon/routes.rs:136-156`
  - claim requires pool state (`db::claim_environment`): `src/db/mod.rs:219-229`

### High

5. **Task-bound environments pass through `pool`, creating steal/race window**
- task envs are marked `pool` after prepare, then later claimed by `run_task`.
- unrelated claim endpoints can claim this env before the task runner does.
- evidence:
  - `finish_preparing_environment` sets `pool`: `src/db/mod.rs:138-153`
  - task prepare enqueues run later: `src/daemon/jobs.rs:119-133`
  - run task claims from pool: `src/daemon/jobs.rs:179`
  - claim endpoints exist and are callable: `src/daemon/routes.rs:211-270`

6. **`claim_next_environment` leaves environment in `in_use` on provider claim failure**
- route claims env in DB first, then runs provider.claim.
- if provider.claim fails, route returns error but env remains `in_use`.
- evidence:
  - DB claim first: `src/daemon/routes.rs:248`
  - provider claim second: `src/daemon/routes.rs:249-252`

7. **Prepare job error handling is inconsistent for task-bound failures**
- task status is only set to failed for certain prepare error paths.
- failures before provider result matching (for example missing env/project, join errors) can fail job without task status update.
- evidence:
  - limited task failure handling in prepare: `src/daemon/jobs.rs:95-116`
  - global failure fallback in `process_job` only handles `run_task`: `src/daemon/jobs.rs:47-68`

8. **Task deletion is not staged end-to-end**
- route enqueues environment removal, then deletes task record immediately.
- if remove job later fails, environment can be stuck without parent task record for diagnosis/retry context.
- evidence: `src/daemon/routes.rs:379-395`

9. **Unbounded job fan-out**
- worker claims all pending jobs then spawns one task per job with no upper bound.
- bursts can oversubscribe resources and amplify failures.
- evidence:
  - claim all pending: `src/db/mod.rs:411-430`
  - unbounded spawn: `src/daemon/jobs.rs:9-15`

### Medium

10. **Queue model lacks idempotency primitives**
- no dedupe keys, attempts, backoff timing, lease expiration, or last-error fields.
- harder to guarantee safe retries and replay.
- evidence: jobs schema `migrations/0001_init.sql:30-37`, db methods `src/db/mod.rs:388-444`

11. **Foreign key behavior is not explicitly enabled per connection**
- schema declares references, but connections do not issue `PRAGMA foreign_keys = ON`.
- integrity enforcement depends on SQLite defaults/environment and is unsafe to assume.
- evidence: `src/db/mod.rs:9-13`; schema refs in `migrations/0001_init.sql:10-28`

12. **API contract is mixed for provider operations**
- some provider operations are `202` staged (`prepare`, `remove`), while others are synchronous `200`.
- inconsistent user expectations and client orchestration complexity.
- evidence: `src/daemon/routes.rs:136-156`, `182-260`, `272-293`, `314-337`

13. **Test coverage does not exercise concurrency/recovery invariants**
- current tests focus on CRUD/lifecycle happy paths plus one failed-prepare case.
- no tests for crash recovery of running jobs, job retry semantics, staging atomicity, or env claim race windows.
- evidence: `tests/daemon.rs`, `tests/database.rs`

## Updated Recommendations

### Architectural Rule

- keep request handlers limited to: validate input, perform single DB transaction to stage state, enqueue jobs, return.
- run all provider/external-process work only in workers.

### State Machine and Queue Changes

1. Introduce explicit task/env staged states:
- task: `pending -> env_preparing -> env_ready -> running -> complete|failed|canceled`
- env (task-bound): `preparing_task -> ready_task -> in_use -> removing -> removed|failed`
- env (pool): `preparing_pool -> pool -> in_use -> removing -> removed|failed`

2. Replace generic `prepare_environment` for task flow with `prepare_task` job:
- payload: `{task_id, env_id}`
- idempotent stage checks before each transition.
- avoid `pool` state for task-bound envs.

3. Add robust queue semantics:
- schema fields: `attempt`, `not_before`, `lease_expires_at`, `dedupe_key`, `last_error`
- claim with `LIMIT N` and short leases.
- retries with bounded backoff.
- startup requeue of expired leases.

### Endpoint and CLI Alignment

1. Convert provider-touching routes to async staged operations (`202`):
- `POST /environments/{id}/update`
- `POST /environments/{id}/claim`
- `POST /environments/claim`

2. Rework `work env create` UX:
- either stage a single create-and-claim operation in one queued job, or
- make it explicit that create returns preparing and claim is separate.

3. Rework task remove:
- stage `cancel_task`/`remove_task` job, keep task record until cleanup reaches terminal state.

### Transactional DB API

- add single-call staging helpers that wrap a transaction:
  - `stage_task_create(project_id, task_provider, env_provider, description)`
  - `stage_env_prepare(project_id, provider)`
  - `stage_env_remove(env_id)`
  - `stage_task_remove(task_id)`

## Prioritized Rollout

### Phase 1 (Correctness / Risk)

1. make provider operations queue-only (`202`) for claim/update endpoints.
2. add transactional staging helper for task create.
3. fix task-bound env race by removing interim `pool` exposure.
4. ensure any prepare failure for task-bound operations marks task terminally failed.

### Phase 2 (Reliability)

1. introduce leases + retry metadata for jobs.
2. claim `LIMIT N` jobs and enforce worker concurrency caps.
3. add startup recovery for stale `running` jobs.

### Phase 3 (Operational Clarity)

1. expand task/env state machine labels.
2. switch hard task delete to staged cancel/archive.
3. optionally add operation resources or structured SSE events.

## Tests to Add

1. transactional atomicity tests for all multi-write staging endpoints.
2. crash recovery test: daemon killed during running job, job is reclaimed/retried.
3. race test: task-bound env cannot be claimed by pool claim endpoints.
4. idempotency tests: duplicate job delivery does not corrupt state.
5. CLI behavior test: `env create` semantics match documented async behavior.

## Conclusion

The system is moving in the right direction for `task new`, but it is still mixed between synchronous provider execution and asynchronous staged execution. The highest-value fixes are: transactional staging at API boundaries, queue-only provider work, and reliable job recovery semantics.
