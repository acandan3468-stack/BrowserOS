# Phase 2 — Architectural Risk Analysis

---

## Risk 1: CDP Dependency Escalation

**Risk:** `browseros-cdp` becomes a monolith. Every new browser feature requires CDP protocol changes. The crate grows unbounded as more CDP domains are implemented.

**Severity:** Medium  
**Likelihood:** High  

**Mitigation:** The bridge layer (`browseros-bridge`) defines the contract. CDP is an implementation detail. If CDP becomes too large, it can be split into sub-crates (`browseros-cdp-dom`, `browseros-cdp-network`, etc.) without changing any consumer.

**Signal:** `browseros-cdp/src/` exceeds 20 modules.

---

## Risk 2: Bridge Trait Proliferation

**Risk:** Every new browser capability requires a new bridge trait, inflating the API surface. Traits become too granular (a trait per method) or too coarse (a trait that violates interface segregation).

**Severity:** Medium  
**Likelihood:** High  

**Mitigation:** Start with 10 bridge traits (BrowserPort, SessionPort, PagePort, FramePort, ElementPort, LocatorPort, NetworkPort, InputPort, StoragePort, DialogPort, DownloadPort). Add new traits only when a capability has 3+ methods with cohesive lifecycle. Use default method implementations for convenience.

**Signal:** More than 15 bridge traits before the first agent release.

---

## Risk 3: Synchronous API Deadlock

**Risk:** CDP operations block the calling thread. If a bridge method is called from an EventBus subscriber (which runs in the publisher's thread), and the publisher thread is also an EventBus dispatcher, all subscribers block. A deadlocked browser operation freezes the entire runtime.

**Severity:** Critical  
**Likelihood:** Medium  

**Mitigation:**
1. **Never call `PagePort::navigate()` (etc.) from EventBus handlers** — this is a design rule enforced by documentation and code review.
2. The CDP reader thread runs independently — CDP event dispatch to EventBus never blocks on a response.
3. A watchdog thread monitors thread liveness and logs warnings if any bridge operation exceeds 30s.

**Signal:** CI detects thread stalls >5s.

---

## Risk 4: CDP Connection State Machine

**Risk:** CDP WebSocket connections can drop, reconnect, or timeout. The state machine for connection lifecycle (connecting → connected → reconnecting → disconnected) interacts poorly with the synchronous API — a method call on a disconnected session blocks forever.

**Severity:** High  
**Likelihood:** Medium  

**Mitigation:**
- `CdpConnection` exposes `is_connected() -> bool`.
- All `*Port` methods check connection state at entry and return `Err(BrowserOsError::transient("browser disconnected"))` if not connected.
- Reconnection is automatic for transient drops (CDP Target.detachedFromTarget → re-attach).
- Permanent disconnection (browser crash) is propagated upward.

---

## Risk 5: Element Handle Staleness

**Risk:** `ElementPort` references become stale when DOM nodes are removed. An agent that holds references across page navigations will operate on dead handles. Silent staleness (element exists but is no longer the intended node) is worse than explicit error.

**Severity:** High  
**Likelihood:** High  

**Mitigation:**
- `ElementId` includes a CDP backend node ID (incremented on each DOM mutation).
- Any bridge method on a stale element returns `Err(BrowserOsError::resource("ELEMENT_DETACHED"))`.
- Agents are expected to re-query after navigation.
- A diagnostic method `ElementPort::is_stale() -> bool` allows proactive checks.

**Accepted risk:** Agents must handle stale references. This is explicit by design.

---

## Risk 6: Thread Explosion in CDP Event Handling

**Risk:** CDP fires thousands of events per second (network traffic, DOM mutations). Each event currently dispatches on the reader thread. If EventBus subscribers are slow, the reader thread backs up, delaying CDP command responses.

**Severity:** Medium  
**Likelihood:** Medium  

**Mitigation:**
- CDP events are high-level synthesized, not raw protocol events.
- High-frequency events (network data received, DOM mutations) are **throttled** before dispatch.
- The CDP reader thread has a bounded channel (10,000 events). When full, older events are dropped.

---

## Risk 7: Config Proliferation

**Risk:** Each Phase 2 crate adds config structs. `RootConfig` must know about all of them. New crate → new config namespace → new `register<T>()` calls. Config becomes a bottleneck.

**Severity:** Low  
**Likelihood:** High  

**Mitigation:** Phase 1 already supports plugin-style config registration:

```rust
let artifact_cfg: ArtifactConfig = config.for_component().unwrap_or_default();
```

The namespace pattern (`artifact.base_path`, `browser.chrome_path`, etc.) prevents key collisions. No `RootConfig` changes are needed for new config blocks.

---

## Risk 8: Cross-Platform Browser Paths

**Risk:** Chrome/Chromium executable paths differ across OS (Windows: `C:\Program Files\Google\Chrome\Application\chrome.exe`, macOS: `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`, Linux: `/usr/bin/google-chrome`). Hardcoding paths breaks on different systems.

**Severity:** Medium  
**Likelihood:** High  

**Mitigation:**
- `browseros-bridge` defines `BrowserLocator` trait for platform detection.
- `browseros-cdp` provides platform-specific implementations.
- Config override via `browser.chrome_path` and `browser.channel` (stable, beta, dev, canary).
- Auto-detection fallback chain: config → `CHROME_PATH` env → `which`/`where` → common paths.

---

## Risk 9: Plugin Isolation Leakage

**Risk:** A WASM or native plugin can still access host resources through EventBus flooding or capability escalation. A malicious plugin could subscribe to all events and exfiltrate data.

**Severity:** High  
**Likelihood:** Low  

**Mitigation:**
- WASM plugins run in a sandbox with no host file system access beyond `data_dir`.
- Native plugins are trusted (same trust model as the OS — only load plugins from configured paths).
- Event subscription rate limiting is a future concern (Phase 3).
- Config path `plugin.scan_path` is blocked from env var injection (Phase 1 security rule).

---

## Risk 10: Async CDP vs Sync API Mismatch

**Risk:** Some CDP operations are inherently asynchronous (Page.printToPDF can take 10+ seconds for complex pages). The synchronous API blocks the caller's thread for the duration. A busy agent that calls `pdf()` in the main thread freezes until PDF finishes.

**Severity:** Medium  
**Likelihood:** Medium  

**Mitigation:**
- The synchronous API blocks the calling thread, not the runtime. Agent threads can be independent from control threads.
- Long operations (>5s) log warnings with the calling thread name.
- Future: add an `async` feature flag that exposes `async fn pdf_async(...)` variants.

**Accepted risk:** Agents manage their own threading. The runtime does not provide async primitives.

---

## Risk 11: CDP Protocol Version Drift

**Risk:** Chrome updates break CDP commands or event shapes. The crate hardcodes protocol types that become stale.

**Severity:** Medium  
**Likelihood:** Medium  

**Mitigation:**
- CDP types are generated from the browser protocol schema (PDL) where possible.
- CI runs integration tests against stable, beta, and canary Chrome channels.
- Unknown CDP fields are captured via `serde(deny_unknown_fields) = false` on response types.

---

## Risk 12: Memory Growth from Event Accumulation

**Risk:** Network capture or DOM snapshot events accumulate in subscriber memory. A long-running agent that subscribes to `network.request_started` will accumulate all HTTP requests, eventually exhausting memory.

**Severity:** High  
**Likelihood:** Medium  

**Mitigation:**
- `NetworkCapture` is an explicit opt-in, not a default subscription.
- Event subscribers are responsible for their own memory management.
- Documentation warns: "Do not accumulate unbounded event streams in subscriber closures."
- `ArtifactStore::prune_older_than()` provides retention management for stored artifacts.

---

## Risk Summary

| ID | Risk | Severity | Likelihood | Mitigation |
|----|------|----------|------------|------------|
| R1 | CDP monolith | Medium | High | Bridge layer, future sub-crate split |
| R2 | Trait proliferation | Medium | High | Start small, add cohesively |
| R3 | Sync API deadlock | Critical | Medium | Design rule, watchdog thread |
| R4 | Connection state | High | Medium | State check on every method |
| R5 | Stale element handles | High | High | Explicit stale errors |
| R6 | CDP event flooding | Medium | Medium | Bounded channel, event throttling |
| R7 | Config proliferation | Low | High | Namespace pattern, zero RootConfig changes |
| R8 | Cross-platform paths | Medium | High | Auto-detection + config override |
| R9 | Plugin isolation leak | High | Low | WASM sandbox, trusted native, config control |
| R10 | Sync/async mismatch | Medium | Medium | Thread separation, warning on long ops |
| R11 | CDP version drift | Medium | Medium | Schema-driven types, multi-channel CI |
| R12 | Event memory growth | High | Medium | Explicit opt-in, documentation, pruning |

**Highest priority mitigations:** R3 (sync deadlock — design rule + watchdog), R4 (connection state), R5 (stale handles), R12 (event memory).
