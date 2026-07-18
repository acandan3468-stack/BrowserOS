# Session Sequence Diagrams

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  

---

## 1. Session Creation and Acquisition (Happy Path)

```
MCP Client      SessionManager      BrowserPool      EventBus        Scheduler
    │                  │                 │               │               │
    │ create(config)   │                 │               │               │
    │─────────────────→│                 │               │               │
    │                  │ publish(session.created)        │               │
    │                  │────────────────→│               │               │
    │                  │                 │               │               │
    │                  │ start resources  │               │               │
    │                  │ acquire(session) │               │               │
    │                  │────────────────→│               │               │
    │                  │   │◄─launch browser─│           │               │
    │                  │   │ BrowserLease   │           │               │
    │                  │◄────────────────│               │               │
    │                  │                 │               │               │
    │                  │ publish(session.starting)       │               │
    │                  │────────────────→│               │               │
    │                  │                 │               │               │
    │                  │ schedule idle timer             │               │
    │                  │─────────────────────────────────→              │
    │                  │                 │               │               │
    │                  │ publish(session.ready)          │               │
    │                  │────────────────→│               │               │
    │◄────SessionId────│                 │               │               │
    │                  │                 │               │               │
    │ acquire(id)      │                 │               │               │
    │─────────────────→│                 │               │               │
    │                  │ lock SessionInner               │               │
    │                  │ state: Ready → Busy             │               │
    │                  │ unlock SessionInner             │               │
    │                  │ update last_activity            │               │
    │                  │                 │               │               │
    │                  │ publish(session.busy)           │               │
    │                  │────────────────→│               │               │
    │                  │                 │               │               │
    │◄──SessionGuard───│                 │               │               │
    │                  │                 │               │               │
    │ [use browser]    │                 │               │               │
    │                  │                 │               │               │
    │ release (drop guard)               │               │               │
    │─────────────────────────────────────│               │               │
    │                  │                 │               │               │
    │                  │ lock SessionInner               │               │
    │                  │ state: Busy → Ready             │               │
    │                  │ unlock SessionInner             │               │
    │                  │ reset idle timer                │               │
    │                  │─────────────────────────────────→              │
    │                  │                 │               │               │
    │                  │ publish(session.idle) (if timeout)             │
```

---

## 2. Session Destruction (Force)

```
MCP Client      SessionManager      BrowserPool      EventBus        Scheduler
    │                  │                 │               │               │
    │ destroy(id, force=true)           │               │               │
    │─────────────────→│                 │               │               │
    │                  │                 │               │               │
    │                  │ [session is Busy]──┐            │               │
    │                  │ Cancel active task │            │               │
    │                  │ state: Busy → Closing          │               │
    │                  │                 │               │               │
    │                  │ cancel idle timer               │               │
    │                  │─────────────────────────────────→              │
    │                  │                 │               │               │
    │                  │ publish(session.closing)        │               │
    │                  │────────────────→│               │               │
    │                  │                 │               │               │
    │                  │ release resources in order      │               │
    │                  │                 │               │               │
    │                  │ release(browser) │               │               │
    │                  │────────────────→│               │               │
    │                  │   │ browser returned to pool  │               │
    │                  │   │ or closed if over max_idle │               │
    │                  │◄────────────────│               │               │
    │                  │                 │               │               │
    │                  │ release(network) (to NetManager)│               │
    │                  │ release(storage) (to StorageMgr)│               │
    │                  │ unsubscribe events               │               │
    │                  │                 │               │               │
    │                  │ state: Closing → Closed         │               │
    │                  │                 │               │               │
    │                  │ publish(session.closed)         │               │
    │                  │────────────────→│               │               │
    │                  │                 │               │               │
    │◄─────Ok─────────│                 │               │               │
```

---

## 3. Idle Timeout → Suspended

```
SessionInner      Scheduler         SessionManager      BrowserPool
    │                  │                  │                 │
    │                  │ [idle timer fires]│                │
    │                  │──────────────────→                │
    │                  │                  │                 │
    │                  │ enqueue(CheckIdle)│                │
    │                  │                  │                 │
    │ [GC runs or acquire/release called]  │                │
    │                  │                  │                 │
    │ lock SessionInner│                  │                 │
    │ state: Ready → Idle (if Ready)      │                 │
    │                  │                  │                 │
    │ [if still idle after suspend_timeout]│                │
    │ state: Idle → Suspended             │                 │
    │                  │                  │                 │
    │ release browser   │                  │                 │
    │ save reservation token              │                 │
    │                  │  release(lease)   │                 │
    │                  │─────────────────→│                 │
    │                  │  │ wait_queue notified          │
    │                  │  │ browser returned to Idle     │
    │                  │◄────────────────│                │
    │                  │                  │                 │
    │ unlock SessionInner                 │                 │
    │                  │                  │                 │
    │                  │ publish(session.suspended)        │
    │                  │────────────────→│                 │
```

---

## 4. Resilient Browser Crash Recovery

```
SessionGuard        SessionManager      BrowserPool      HealthChecker
    │                    │                  │                 │
    │ resource::<Browser>()                │                 │
    │───────────────────→│                  │                 │
    │                    │ [CDP command fails]               │
    │                    │                  │                 │
    │                    │  ◄────health check failure───     │
    │                    │                  │                 │
    │                    │ notify browser dead               │
    │                    │◄────────────────│                 │
    │                    │                  │                 │
    │                    │ publish(browser.crashed)          │
    │                    │                  │                 │
    │                    │ [acquire new browser]             │
    │                    │────────────────→│                 │
    │                    │  │ launch replacement            │
    │                    │◄─BrowserLease───│                 │
    │                    │                  │                 │
    │                    │ update session resource           │
    │                    │                  │                 │
    │◄───new browser────│                  │                 │
    │                    │                  │                 │
    │ [retry CDP command]                   │                 │
```

---

## 5. Graceful Shutdown

```
ShutdownSignal     SessionManager     [Busy Sessions]   [Idle Sessions]   BrowserPool
    │                    │                  │                  │              │
    │ shutdown(timeout)  │                  │                  │              │
    │───────────────────→│                  │                  │              │
    │                    │                  │                  │              │
    │                    │ lock Registry    │                  │              │
    │                    │ set shutting_down = true           │              │
    │                    │ unlock Registry  │                  │              │
    │                    │                  │                  │              │
    │                    │ for each idle session:             │              │
    │                    │─────────────────→│                  │              │
    │                    │   destroy(force=false)             │              │
    │                    │   state: Idle → Closing → Closed   │              │
    │                    │   resources released               │              │
    │                    │◄────────────────│                  │              │
    │                    │                  │                  │              │
    │                    │ for each busy session:             │              │
    │                    │─────────────────→│                  │              │
    │                    │   send cancellation signal         │              │
    │                    │   start wait timer                 │              │
    │                    │◄────────────────│                  │              │
    │                    │                  │                  │              │
    │                    │ [wait_timeout elapsed]              │              │
    │                    │                  │                  │              │
    │                    │ for remaining busy sessions:       │              │
    │                    │─────────────────→│                  │              │
    │                    │   destroy(force=true)              │              │
    │                    │   state: Busy → Closing → Closed   │              │
    │                    │   resources force-released         │              │
    │                    │◄────────────────│                  │              │
    │                    │                  │                  │              │
    │                    │ close all remaining browsers       │              │
    │                    │───────────────────────────────────→│              │
    │                    │◄───────────────────────────────────│              │
    │                    │                  │                  │              │
    │                    │ publish(shutdown.complete)         │              │
    │                    │                  │                  │              │
    │◄──ShutdownReport──│                  │                  │              │
```

---

## 6. MCP Client with Multiple Sessions

```
MCP Client          SessionManager      BrowserPool
    │                     │                 │
    │ create(config_1)    │                 │
    │────────────────────→│                 │
    │◄───session_id_1────│                 │
    │                     │ acquire browser │
    │                     │────────────────→│
    │                     │◄─BrowserLease──│
    │                     │                 │
    │ create(config_2)    │                 │
    │────────────────────→│                 │
    │◄───session_id_2────│                 │
    │                     │ acquire browser │
    │                     │────────────────→│
    │                     │◄─BrowserLease──│
    │                     │                 │
    │ acquire(session_id_1)                 │
    │────────────────────→│                 │
    │◄───SessionGuard_1──│                 │
    │                     │                 │
    │ acquire(session_id_2)                 │
    │────────────────────→│                 │
    │◄───SessionGuard_2──│                 │
    │                     │                 │
    │ [parallel work in both sessions]      │
    │                     │                 │
    │ drop guard_1        │                 │
    │────────────────────→│                 │
    │                     │ browser_1 idle  │
    │                     │                 │
    │ drop guard_2        │                 │
    │────────────────────→│                 │
    │                     │ browser_2 idle  │
```

---

## 7. Reconnection (Session Survives Client Disconnect)

```
MCP Client (A)    SessionManager      BrowserPool        MCP Client (B)
    │                  │                 │                    │
    │ create()         │                 │                    │
    │─────────────────→│                 │                    │
    │◄─session_id X───│                 │                    │
    │                  │                 │                    │
    │ [Client A disconnects]             │                    │
    │                                                                
    │                  │ [session X idle, browser retained]   │
    │                  │ idle timer starts                   │
    │                  │                 │                    │
    │                  │                              Client B connects
    │                  │                              with session_id X
    │                  │◄──────────────────────────────────────│
    │                  │ verify token/session owner           │
    │                  │ update client_id → Client B          │
    │                  │ reset idle timer                     │
    │                  │ acquire allowed                      │
    │                  │─────────────────────────────────────→│
    │                  │                              SessionGuard
```

Protocol: MCP client passes `session_id` + `session_token` (opaque string returned by create). On reconnection, the session owner is verified by matching the token. Token can be transferred explicitly (`session.transfer(new_client_id, new_token)`).

---

## 8. Resource Transfer Between Sessions

```
Session A          SessionManager      ResourceProvider    Session B
    │                    │                  │                  │
    │ transfer(browser, target=Session B)  │                  │
    │───────────────────→│                  │                  │
    │                    │                  │                  │
    │                    │ verify Session A owns browser      │
    │                    │ verify Session B can accept        │
    │                    │                  │                  │
    │                    │ unbind browser from A              │
    │                    │ bind browser to B                  │
    │                    │                  │                  │
    │                    │ update lease owner                 │
    │                    │────────────────→│                  │
    │                    │   lease.claimed_by = B             │
    │                    │◄────────────────│                  │
    │                    │                  │                  │
    │                    │ publish(resource.transferred)       │
    │◄────Ok───────────│                  │                  │
    │                    │                  │                  │
    │                    │         Session B can now use browser
```

Resource transfer is an explicit operation. It fails if:
- Source session does not own the resource
- Target session is at its resource limit
- Resource type does not support transfer
