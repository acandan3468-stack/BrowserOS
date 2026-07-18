# DAG Engine Test Plan — browseros-dag

**Status:** DESIGN  
**Date:** 2026-07-13  

---

## 1. Test Categories

| Category | Count | Priority | Runtime |
|----------|-------|----------|---------|
| Unit tests (per module) | ~25 | Critical | < 1s |
| Integration tests (cross-module) | ~10 | Critical | < 2s |
| Stress tests (high load) | ~5 | High | < 10s |
| Property-based tests | ~5 | Medium | < 5s |
| **Total** | **~45** | | |

---

## 2. Unit Tests

### 2.1 Graph Module (graph.rs) — 8 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `empty_graph_no_error` | Empty graph topo-sort succeeds | INV-001 |
| `single_node_sort` | Single node produces one layer | INV-006 |
| `linear_chain_sort` | A→B→C produces [A],[B],[C] | INV-006 |
| `parallel_nodes_sort` | A→C, B→C produces [A,B],[C] | INV-007 |
| `diamond_sort` | A→B, A→C, B→D, C→D | INV-006, INV-007 |
| `cycle_detected` | A→B, B→A returns CycleDetected | INV-001 |
| `self_cycle` | A→A returns CycleDetected | INV-001 |
| `complex_dag` | 10-node DAG with branching | INV-006 |

### 2.2 Engine Module (engine.rs) — 8 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `execute_single_node` | Single node executes and completes | INV-006 |
| `execute_linear_dag` | A→B executes A then B | INV-006 |
| `execute_diamond_dag` | Parallel branches both execute | INV-007 |
| `execute_with_retry_success` | Node succeeds on retry | INV-011, INV-012 |
| `execute_with_retry_exhausted` | Node fails after retries exhausted | INV-011 |
| `execute_with_timeout` | Node exceeding timeout fails | INV-014 |
| `cancel_mid_execution` | Cancel stops running DAG | INV-015, INV-016 |
| `execute_empty_nodes` | Empty node list succeeds immediately | INV-001 |

### 2.3 Node Module (node.rs) — 4 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `command_node_creation` | Command node wraps closure | INV-004 |
| `sub_dag_node_creation` | SubDag node wraps DagEngine | INV-004 |
| `retry_policy_attachment` | RetryPolicy stored on node | INV-011 |
| `node_id_uniqueness` | Same NodeId on register errors | INV-004 |

### 2.4 Error Module (error.rs) — 2 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `dag_error_display` | All variants display correctly | INV-025 |
| `dag_error_is_retryable` | Transient error classification | INV-013 |

### 2.5 Executor Module (executor.rs) — 3 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `parallel_execution_concurrent` | Independent nodes run in separate threads | INV-007 |
| `upstream_failure_skips_downstream` | Failed node causes skip | INV-010 |
| `skipped_nodes_not_executed` | Skipped nodes never run | INV-010 |

---

## 3. Integration Tests

### 3.1 DAG + EventBus — 3 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `dag_events_emitted_on_start` | ExecutionStarted published on execute() | INV-021 |
| `dag_events_emitted_on_completion` | ExecutionCompleted published | INV-021 |
| `node_events_emitted_on_transition` | Started/Completed/Failed per node | INV-021 |

### 3.2 DAG + Scheduler — 2 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `retry_timing_uses_scheduler` | Retry delay uses schedule_after | INV-012 |
| `cancel_during_retry_delay` | Cancel during retry backoff works | INV-015 |

### 3.3 DAG + Observability — 2 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `metrics_counters_incremented` | nodes.started/completed/failed count | INV-022 |
| `trace_spans_created` | Each node execution traced | INV-023 |

### 3.4 DAG + RuntimeContext — 2 tests

| Test | Description | Verifies |
|------|-------------|----------|
| `dag_accessible_via_runtime` | ctx.dag() returns working engine | Step 3 |
| `dag_registered_in_lifecycle` | "dag_engine" component registered | Step 5 |

---

## 4. Stress Tests

| Test | Description | Duration | Verifies |
|------|-------------|----------|----------|
| `large_dag_execution` | 100-node DAG completes correctly | < 5s | INV-006 |
| `high_parallelism` | 20 parallel nodes execute concurrently | < 3s | INV-007 |
| `concurrent_executions` | 10 simultaneous execute() calls | < 5s | INV-018 |
| `retry_storm` | 10 nodes all retrying simultaneously | < 5s | INV-011 |
| `cancel_during_execution` | Cancel mid-100-node DAG | < 3s | INV-015 |

---

## 5. Property-Based Tests (proptest)

| Test | Description | Verifies |
|------|-------------|----------|
| `arbitrary_dag_sort` | Random DAG topo-sort is valid | INV-001 |
| `cycle_property` | Random edges never produce undetected cycle | INV-001 |
| `retry_policy_property` | Retry delay always <= max_delay | INV-014 |
| `state_transition_property` | Node state transitions are valid | INV-009 |
| `parallelism_property` | Independent nodes never run sequentially | INV-007 |

---

## 6. Edge Cases

| Scenario | Expected Behavior |
|----------|------------------|
| Empty node list | Execute succeeds immediately |
| Single node | Execute completes in one step |
| 1000-node linear chain | Execute completes (performance test) |
| Diamond with 100 parallel nodes | All parallel nodes execute independently |
| Node returning permanent error | Immediate failure, no retry |
| Node panicking | Panic caught, returned as DagError::ExecutionFailed |
| CancellationToken already cancelled | Execution returns immediately with Cancelled state |
| Register node during execution | Error (graph locked) |
| Add edge to nonexistent node | DagError::NodeNotFound |
| Duplicate node registration | DagError::NodeAlreadyExists |
| SubDAG node where inner DAG fails | Outer DAG receives failure |

---

## 7. Test File Structure

```
browseros-dag/
├── src/
│   ├── engine.rs    (unit tests inline)
│   ├── graph.rs     (unit tests inline)
│   ├── node.rs      (unit tests inline)
│   ├── error.rs     (unit tests inline)
│   └── executor.rs  (unit tests inline)
├── tests/
│   ├── dag_integration.rs     # EventBus + Scheduler integration
│   ├── dag_observability.rs   # Metrics + Tracer integration
│   ├── dag_runtime.rs         # RuntimeContext integration
│   ├── dag_stress.rs          # Large DAG + high parallelism
│   └── dag_proptest.rs        # Property-based tests
```

---

## 8. Test Fixtures

```rust
// Common test utilities
mod test_helpers {
    use super::*;
    
    pub fn test_dag() -> DagEngine { /* ... */ }
    
    pub fn ok_node(id: &str) -> DagNode {
        let id: NodeId = id.parse().unwrap();
        DagNode::command(id, id.to_string(), Box::new(|| Ok(())))
    }
    
    pub fn fail_node(id: &str) -> DagNode {
        let id: NodeId = id.parse().unwrap();
        DagNode::command(id, id.to_string(), Box::new(|| {
            Err(DagError::ExecutionFailed {
                node_id: id,
                reason: "intentional failure".into(),
            })
        }))
    }
    
    pub fn retry_node(id: &str, succeeds_on: Arc<AtomicU32>) -> DagNode {
        let id: NodeId = id.parse().unwrap();
        let sid = id.to_string();
        DagNode::command(id.clone(), id.to_string(), Box::new(move || {
            let attempt = succeeds_on.fetch_sub(1, Ordering::SeqCst);
            if attempt <= 1 { Ok(()) }
            else { Err(DagError::ExecutionFailed {
                node_id: id.clone(),
                reason: format!("attempt {}", attempt),
            })}
        }))
        .with_retry(RetryPolicy::Immediate { max_retries: 5 })
    }
}
```

---

## 9. Success Criteria

- [ ] All 45+ tests pass
- [ ] `cargo test -p browseros-dag` — 0 failures
- [ ] `cargo clippy -p browseros-dag` — 0 warnings
- [ ] `cargo build --workspace` — 0 errors
- [ ] `cargo test --workspace` — all existing tests still pass
- [ ] Coverage > 80% (measured by `cargo tarpaulin` or similar)
- [ ] No unwrap()/expect()/panic!() in production code
