# Phase 6 Preparation — MCP Architecture Migration

**Tarih:** 2026-07-16  
**Kapsam:** Mevcut `browseros-llm` MCP server'ından yeni `browseros-mcp` mimarisine geçiş için yapılması gereken değişiklikler  
**Öncelik:** Bu doküman kod değişikliği İÇERMEZ. Sadece yapılması gerekenleri listeler.

---

## 1. Mevcut Durum Özeti

```
browseros-llm/
├── src/bin/llm_gateway_server.rs    ← MCP Server (3 tool: chat, embed, health)
├── src/mcp/                         ← MCP Client (McpAdapter, transport, client)
└── src/gateway.rs                   ← LlGateway (LLM provider orchestration)
```

**Sorun:** `llm_gateway_server.rs` sadece LLM yetenekleri sunar. BrowserOS'un tarayıcı, DOM, DAG, network, storage gibi temel yeteneklerini expose edemez.

---

## 2. Phase 6 İçin Gerekli Adımlar

### 2.1 `browseros-mcp` Crate'inin Oluşturulması

Yeni bir crate eklenir:

```
browseros-mcp/
├── Cargo.toml
└── src/
    ├── bin/
    │   └── browseros_mcp_server.rs      ← Ana MCP server binary
    ├── server/
    │   ├── mod.rs                        ← McpServer, start(), shutdown()
    │   ├── transport.rs                  ← stdio handler (llm_gateway_server.rs'den taşı)
    │   ├── handlers.rs                   ← JSON-RPC metod dispatcher
    │   └── session.rs                    ← SessionManager, SessionState
    ├── tools/
    │   ├── mod.rs                        ← McpTool trait, ToolRegistry
    │   ├── system.rs                     ← health, config, metrics, version
    │   ├── session.rs                    ← create, close, list
    │   ├── llm.rs                        ← models (→ browseros-llm)
    │   ├── browser.rs                    ← navigate, screenshot, evaluate
    │   ├── dom.rs                        ← query, click, type, snapshot
    │   ├── network.rs                    ← intercept, conditions
    │   ├── workflow.rs                   ← execute, plan, status
    │   └── storage.rs                    ← cookies, localstorage
    ├── events/
    │   └── mod.rs                        ← Notification dispatcher (EventBus → MCP)
    └── types.rs                          ← SessionId, ToolResult, ortak tipler
```

### 2.2 Cargo.toml Bağımlılıkları

```toml
[package]
name = "browseros-mcp"
version = "0.1.0"
edition = "2021"

[dependencies]
browseros-runtime           # RuntimeContext → tüm alt sistemlere erişim
browseros-llm               # LlGateway → chat, embed
browseros-bridge            # BrowserPort, DomPort trait'leri
browseros-types             # HandleId, EventType
browseros-config            # RootConfig
browseros-observability     # Logger, MetricsRegistry
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
```

**Önemli:** `browseros-mcp` doğrudan `browseros-browser`, `browseros-cdp`, `browseros-dom` gibi alt seviye crate'lere bağımlı DEĞİLDİR. Tüm erişim `browseros-runtime::RuntimeContext` üzerinden yapılır.

### 2.3 Workspace Kaydı

`browseros/Cargo.toml`'a `browseros-mcp` üye olarak eklenir:

```toml
[workspace]
members = [
    "browseros-types",
    "browseros-config",
    "browseros-observability",
    "browseros-event-bus",
    "browseros-lifecycle",
    "browseros-scheduler",
    "browseros-runtime",
    "browseros-dag",
    "browseros-bridge",
    "browseros-browser",
    "browseros-cdp",
    "browseros-page",
    "browseros-dom",
    "browseros-storage",
    "browseros-llm",
    "browseros-mcp",          # ← YENİ
]
```

---

## 3. Mevcut Koddan Taşınacak Bileşenler

### 3.1 Taşınacaklar

| Bileşen | Kaynak | Hedef | Açıklama |
|---------|--------|-------|----------|
| JSON-RPC handler döngüsü | `llm_gateway_server.rs:22-128` | `server/handlers.rs` | Stdio okuma, metod dispatch, response yazma |
| `respond()` fonksiyonu | `llm_gateway_server.rs:248-252` | `server/transport.rs` | JSON-RPC response yazma |
| Tool schema tanımları | `llm_gateway_server.rs:53-104` | `tools/system.rs` | `system/health` tool'u, `system/version` |
| Init handshake | `llm_gateway_server.rs:42-51` | `server/handlers.rs` | `initialize` → `serverInfo` |
| Error handling | `llm_gateway_server.rs:30-37, 113-126` | `server/handlers.rs` | Parse error, unknown method/tool |

### 3.2 KALACAKLAR (değişiklik yok)

| Bileşen | Yer | Açıklama |
|---------|-----|----------|
| `McpAdapter` | `browseros-llm/src/mcp/adapter.rs` | MCP Client → LlProvider köprüsü. `LlGateway`'in harici MCP tool'larını provider olarak görmesini sağlar. |
| `McpClient` | `browseros-llm/src/mcp/client.rs` | MCP istemci — JSON-RPC initialize, list_tools, call_tool. `McpAdapter` tarafından kullanılır. |
| `Transport` trait + `StdioTransport` | `browseros-llm/src/mcp/transport.rs` | Subprocess yönetimi, stdio okuma/yazma. `McpClient` tarafından kullanılır. |
| JSON-RPC tipleri | `browseros-llm/src/mcp/jsonrpc.rs` | JsonRpcRequest/Response/Error, McpToolDef, MCP'ye özel yapılar. `McpClient` + yeni server tarafından kullanılır. |
| `McpError` | `browseros-llm/src/mcp/errors.rs` | Hata tipleri. `McpClient` + yeni server tarafından kullanılır. |
| `LlGateway` | `browseros-llm/src/gateway.rs` | LLM provider orchestration. Yeni server `browseros-llm::LlGateway`'i import eder. |
| `LlProvider`, `McpAdapter` | `browseros-llm/` | Hiçbir değişiklik gerekmez. |

### 3.3 Paylaşılan Kod

`browseros-llm/src/mcp/jsonrpc.rs` ve `browseros-llm/src/mcp/errors.rs` hem MCP Client (`McpAdapter`) hem de yeni MCP Server (`browseros-mcp`) tarafından kullanılır. İki seçenek:

**A seçeneği (Önerilen):** Bu tipleri `browseros-llm`'den re-export et. `browseros-mcp`, `browseros-llm::mcp::jsonrpc` ve `browseros-llm::mcp::errors`'i kullanır.

**B seçeneği:** Ortak tipleri `browseros-types`'a taşı. Daha temiz dependency grafiği ancak daha fazla değişiklik.

**Öneri:** Phase 6'da **A seçeneği** kullanılsın. Phase 8'de gerekirse B'ye geçilsin.

---

## 4. RuntimeContext'in MCP İçin Genişletilmesi

`browseros-runtime::RuntimeContext`'in MCP server'ın erişmesi gereken tüm alt sistemleri expose etmesi gerekir. Mevcut durum:

```rust
// browseros-runtime/src/lib.rs
pub struct RuntimeContext {
    pub bus: Arc<EventBus>,
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsRegistry>,
    pub tracer: Arc<Tracer>,
    pub config: Arc<RootConfig>,
    pub lifecycle: Arc<LifecycleManager>,
    pub scheduler: Arc<Scheduler>,
    pub dag: Arc<DagEngine>,
}
```

Phase 6 için `RuntimeContext`'e eklenmesi gerekenler:

```rust
// browseros-runtime — Phase 6 genişletmesi
pub struct RuntimeContext {
    // Mevcut
    pub bus: Arc<EventBus>,
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsRegistry>,
    pub tracer: Arc<Tracer>,
    pub config: Arc<RootConfig>,
    pub lifecycle: Arc<LifecycleManager>,
    pub scheduler: Arc<Scheduler>,
    pub dag: Arc<DagEngine>,

    // Phase 6 — YENİ
    pub llm: Arc<LlGateway>,            // browseros-llm
    pub browser_pool: Arc<BrowserPool>, // browseros-browser
}
```

`BrowserPool` ve `LlGateway`, `RuntimeContext::init()` içinde oluşturulur veya dışarıdan enjekte edilir.

---

## 5. Mevcut `llm_gateway_server.rs`'in Geleceği

| Rol | Karar |
|-----|-------|
| Production MCP sunucusu | ❌ Kullanılmayacak. Yerine `browseros-mcp-server` geçecek. |
| Diagnostic/test aracı | ✅ Kalacak. `cargo run --bin llm_gateway_server` ile LLM bağlantısı test edilebilir. |
| CI/CD validation | ✅ Kalacak. `PHASE5B` testleri `llm_gateway_server.exe`'yi kullanmaya devam edebilir. |
| Geliştirici aracı | ✅ Kalacak. Hızlı LLM testleri için kullanışlı. |

**Hiçbir kod silinmeyecek.** Sadece yeni binary eklenecek.

---

## 6. Phase 6 Teslimat Kriterleri

| # | Kriter | Doğrulama |
|---|--------|-----------|
| 1 | `browseros-mcp` crate'i oluşturuldu | `cargo build` geçiyor |
| 2 | `browseros_mcp_server` binary'si çalışıyor | `cargo run --bin browseros_mcp_server` |
| 3 | `system/health` tool'u eski health ile aynı çıktıyı üretiyor | E2E test |
| 4 | `system/version` tool'u versiyon bilgisi döndürüyor | E2E test |
| 5 | `session/create` ve `session/close` çalışıyor | E2E test |
| 6 | MCP initialize + tools/list + tools/call döngüsü çalışıyor | 5B testleri adapte edildi |
| 7 | Mevcut 595 test hala geçiyor | `cargo test` |
| 8 | `llm_gateway_server.exe` hala çalışıyor (diagnostic) | Ayrı binary olarak var |
| 9 | Loglar stderr'e gidiyor (stdout JSON-RPC için temiz) | E2E test |
| 10 | Session isolation: iki client aynı anda çalışabiliyor | E2E test |

---

## 7. Riskler ve Mitigasyonlar

| Risk | Olasılık | Etki | Mitigasyon |
|------|----------|------|------------|
| `browseros-mcp` ve `browseros-llm` arasında dairesel bağımlılık | Düşük | Yüksek | `browseros-mcp` sadece `browseros-llm`'e bağlanır, tersi olmaz. JSON-RPC tipleri `browseros-llm`'de kalır. |
| RuntimeContext god object haline gelir | Orta | Orta | MCP sadece `runtime.llm` ve `runtime.browser_pool` alanlarına erişir. Yeni alanlar eklendikçe erişim pattern'i korunur. |
| Session yönetimi karmaşıklaşır | Orta | Düşük | Her session kendi `BrowserPool` instance'ını alır. SessionManager sadece bir HashMap<String, Session>'dir. |
| Mevcut MCP client'lar (OpenCode) eski tool isimlerini kullanıyordur | Düşük | Düşük | Mevcut `llm_gateway_server` hala çalışır. Yeni server farklı bir binary adıyla gelir. |

---

*Bu doküman mevcut implementasyonu değiştirmez. Phase 6 için yapılacakları listeler.*
