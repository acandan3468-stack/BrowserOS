# MCP Architecture Review — browseros-llm

**Tarih:** 2026-07-16  
**Kapsam:** Mevcut `llm_gateway_server.rs` tasarımının mimari değerlendirmesi, BrowserOS uzun vadeli vizyonuyla uyum analizi  
**Yöntem:** Kod incelemesi, crate dependency analizi, faz planlarıyla karşılaştırma  

---

## 1. Temel Soru: browseros-llm MCP Server Olmalı mı?

### Mevcut Durum

`browseros-llm` içinde iki MCP rolü birden bulunuyor:

| Rol | Dosya | Açıklama |
|-----|-------|----------|
| **MCP Server** | `src/bin/llm_gateway_server.rs` | Diğer MCP client'larına hizmet veren bir sunucu. 3 tool: `chat`, `embed`, `health` |
| **MCP Client** | `src/mcp/` (adapter, client, transport) | Harici MCP sunucularına bağlanan bir istemci. `LlProvider` trait'ini implemente ederek LlGateway'e entegre olur |

### Değerlendirme

**MCP Client (`mcp::` modülü) — DOĞRU YERDE.**  
`McpAdapter` bir `LlProvider` implementasyonudur. LLM Gateway'in harici MCP tool'larını bir provider olarak görmesi mantıklıdır. Bu modül `browseros-llm` içinde kalmalıdır.

**MCP Server (`llm_gateway_server.rs`) — YANLIŞ YERDE.**  
Bu binary, BrowserOS'un tamamını değil sadece LLM yeteneklerini expose eder. Nedenleri:

1. **Adında çelişki:** "LLM Gateway" adıyla anılan bir crate, tarayıcı otomasyonu, DOM sorgulama, DAG workflow yürütme gibi yetenekleri expose etmemelidir. Bu sorumlulukların tamamı bu crate'e yüklenirse `browseros-llm` bir "god crate" haline gelir.

2. **Dependency yok:** `browseros-llm`, `browseros-bridge`, `browseros-browser`, `browseros-cdp`, `browseros-dom`, `browseros-page`, `browseros-dag` gibi crates'lerin hiçbirine bağımlı değildir. Bu bağımlılıkları eklemek, LLM crate'inin sorumluluk alanını ihlal eder.

3. **İkinci bir MCP sunucusu kaçınılmaz:** Gelecekte tarayıcı ve workflow yetenekleri eklendiğinde ya `browseros-llm` devleşecek ya da ikinci bir MCP server binary'si yazılacak. Her iki senaryo da kötü.

4. **Kafa karıştırıcı isimlendirme:** `llm_gateway_server` adı, MCP dünyasında "LLM sağlayan sunucu" imajı verir. Oysa BrowserOS'un MCP sunucusu "BrowserOS'un kendisi" olmalıdır — LLM onun sadece bir alt yeteneğidir.

### Karar

> **browseros-llm MCP Server olarak KALMAMALIDIR.**  
> Mevcut `llm_gateway_server.rs` bir **diagnostik/test aracı** olarak korunabilir, ancak üretim MCP sunucusu olarak kullanılmamalıdır.

---

## 2. MCP Server Hangi Crate İçinde Yaşamalı?

### Adaylar

| Crate | Bağımlılıklar | Erişebildiği Alt Sistemler | Değerlendirme |
|-------|--------------|---------------------------|---------------|
| `browseros-llm` | types, event-bus, observability | Sadece LLM provider'ları | ❌ LLM dışı yeteneklere erişemez |
| `browseros-runtime` | types, config, observability, event-bus, lifecycle, scheduler, dag | Tüm alt sistemler (LLM hariç) | ❌ LLM provider'larına erişemez |
| `browseros-agent` (yok) | Yok — henüz oluşturulmadı | Henüz yok | ❌ Mevcut değil |
| **Yeni: `browseros-mcp`** | runtime (tüm alt sistemlere erişim), llm (LLM yetenekleri) | **Tüm BrowserOS** | ✅ **Önerilen** |

### Önerilen Karar

**Yeni bir `browseros-mcp` crate'i oluşturulmalıdır.**

```
browseros-mcp/
├── Cargo.toml
├── src/
│   ├── bin/
│   │   └── browseros_mcp_server.rs    # Ana MCP server binary
│   ├── server/
│   │   ├── mod.rs                      # McpServer — ana struct
│   │   ├── transport.rs                # stdio transport handler
│   │   ├── handlers.rs                 # JSON-RPC metod dispatcher
│   │   └── session.rs                  # Session state management
│   ├── tools/
│   │   ├── mod.rs                      # Tool trait + registry
│   │   ├── llm.rs                      # chat, embed (→ browseros-llm)
│   │   ├── browser.rs                  # navigate, screenshot, evaluate
│   │   ├── dom.rs                      # query, click, type, observe
│   │   ├── session.rs                  # create, close, list
│   │   ├── workflow.rs                 # execute, plan, status (→ browseros-dag)
│   │   ├── network.rs                  # intercept, mock, har
│   │   ├── system.rs                   # health, config, metrics
│   │   └── storage.rs                  # cookies, localstorage
│   └── types.rs                        # MCP'ye özgü paylaşılan tipler
└── tests/
    └── mcp_integration_test.rs
```

### Bağımlılık Grafiği

```
browseros-mcp
├── browseros-runtime    (DagEngine, LifecycleManager, Scheduler, RuntimeContext)
├── browseros-llm        (LlGateway → chat, embed, health)
├── browseros-bridge     (BrowserPort, DomPort, NetworkPort trait'leri)
├── browseros-types      (HandleId, EventType, ortak tipler)
├── browseros-observability (Logger, MetricsRegistry)
├── browseros-config     (RootConfig)
├── serde / serde_json   (JSON-RPC)
└── chrono               (timestamp'ler)
```

`browseros-mcp`, `browseros-runtime` üzerinden tüm alt sistemlere erişir. `browseros-runtime`'ın `RuntimeContext`'i, tüm BrowserOS bileşenlerine tek bir `Arc` üzerinden erişim sağlar.

---

## 3. Chat Toolunun Gerekliliği

### Mevcut Durum

`llm_gateway_server.rs` bir `chat` tool'u sunar. Bu tool, gelen mesajları `LlGateway::chat()`'e yönlendirir.

### Değerlendirme

Chat tool'unun MCP server'da bulunması **uzun vadede gereksizdir**. Nedenleri:

1. **MCP client'ların kendi LLM'leri var.** OpenCode, Claude Code, Cursor gibi araçlar kendi LLM provider'larını kullanır. BrowserOS'un onlara LLM sağlamasına gerek yoktur — aksine BrowserOS, onların **LLM'lerine ihtiyaç duyar** (tool orchestration için).

2. **BrowserOS LLM'yi içsel olarak kullanır.** BrowserOS, workflow planlama, sayfa analizi, form doldurma gibi görevlerde LLM'yi kendi içinde kullanır. Bu kullanım dışa açık bir MCP tool'u değil, içsel bir yetenektir.

3. **Tool orchestration katmanı.** BrowserOS'un asıl değeri, bir AI agent'ın "şu sayfaya git, şu butona tıkla, sonucu oku" gibi komutlarını MCP tool'ları aracılığıyla yerine getirmesidir. LLM sohbeti bu resmin dışındadır.

4. **İstisna: diagnostics.** Geliştirme ve test sırasında LLM'in çalıştığını doğrulamak için bir `chat` tool'u yararlı olabilir. Ancak bu, production MCP API'sinin bir parçası olmamalıdır.

### Karar

> **Chat tool'u production MCP server'dan KALDIRILMALIDIR.**  
> Diagnostic amaçlı olarak `llm_gateway_server.exe`'de kalabilir, ancak `browseros-mcp`'nin tool listesinde yer almamalıdır.  
> BrowserOS'un LLM ihtiyacı içseldir — `browseros-llm` üzerinden gateway katmanında çözülür.

---

## 4. browseros-llm'in Gelecekteki Rolü

`browseros-llm` aşağıdaki rollerde KALMALIDIR:

1. **LLM Provider Gateway:** OpenAI, Anthropic, Gemini, Ollama gibi provider'ları yönetir. Routing, retry, fallback, caching, cost tracking sağlar.

2. **MCP Client (`McpAdapter`):** Harici MCP sunucularındaki tool'ları `LlProvider` olarak LlGateway'e entegre eder. Bu sayede LLM'ler harici MCP tool'larını çağırabilir.

3. **Crate olarak:** `browseros-mcp`'nin `tools/llm.rs` modülü tarafından kullanılır. `browseros-mcp`, `browseros-llm::LlGateway`'i çağırarak LLM işlemlerini gerçekleştirir, kendisi doğrudan provider'larla konuşmaz.

---

## 5. BrowserOS'un MCP Sunucu Mimarisi

### Nihai Durum (Phase 8)

```
                    ┌──────────────────────────────────────┐
                    │     MCP Client (OpenCode, Claude,    │
                    │      Cursor, VS Code Agent, vb.)     │
                    └──────────────┬───────────────────────┘
                                   │ stdin/stdout (JSON-RPC 2.0)
                                   ▼
┌──────────────────────────────────────────────────────────────────────┐
│ browseros-mcp-server.exe                                             │
│                                                                      │
│  McpServer                                                           │
│  ├── Transport (stdio/WebSocket/SSE)                                │
│  ├── SessionManager (her istemci için izole BrowserOS instance)     │
│  ├── ToolRegistry                                                    │
│  │   ├── tool::session  (create, close, list, config)               │
│  │   ├── tool::browser  (navigate, screenshot, pdf, evaluate)       │
│  │   ├── tool::dom      (query, click, type, select, observe)       │
│  │   ├── tool::network  (intercept, mock, throttle, har)            │
│  │   ├── tool::storage  (cookies, localstorage, indexeddb)           │
│  │   ├── tool::workflow (execute, plan, status, cancel)             │
│  │   ├── tool::system   (health, config, logging, metrics)          │
│  │   └── tool::llm      → browseros-llm::LlGateway                  │
│  └── EventBus subscription → progress notifications                 │
│                                                                      │
│  RuntimeContext (Arc)                                                │
│  ├── LlGateway          (browseros-llm)                              │
│  ├── BrowserPool        (browseros-browser)                          │
│  ├── DomService         (browseros-dom)                              │
│  ├── NetworkService     (browseros-network — Phase 2.6)              │
│  ├── StorageManager     (browseros-storage)                          │
│  ├── DagEngine          (browseros-dag)                              │
│  ├── Scheduler          (browseros-scheduler)                        │
│  ├── LifecycleManager   (browseros-lifecycle)                        │
│  └── EventBus           (browseros-event-bus)                        │
└──────────────────────────────────────────────────────────────────────┘
```

### Temel Prensipler

| Prensip | Açıklama |
|----------|-----------|
| **Tek MCP sunucusu** | Tüm BrowserOS yetenekleri tek bir MCP sunucusu üzerinden dışa açılır. Ayrı ayrı MCP sunucuları (ör. browser-mcp, dom-mcp, workflow-mcp) yoktur. |
| **Session isolation** | Her MCP client'ın kendi BrowserOS session'ı vardır. Session'lar tamamen izole edilmiştir. |
| **LLM içseldir** | BrowserOS LLM'yi kendi içinde kullanır. Dışarıya LLM sohbeti sunmaz. |
| **Tool → Service** | Her tool, arkasındaki BrowserOS service'ine (DomService, NetworkService, vb.) delegasyon yapar. Tool katmanı sadece dönüşüm ve protokol yönetimidir. |
| **Progress notifications** | Uzun süren işlemler (workflow, navigation) için MCP `notifications/progress` kullanılır. |
| **Event subscription** | BrowserOS olayları MCP client'a iletilir (sayfa yüklendi, DOM değişti, ağ isteği tamamlandı). |

---

## 6. Mevcut Tasarımın Güçlü ve Zayıf Yönleri

### Güçlü Yönler

| Özellik | Değerlendirme |
|----------|--------------|
| **Temiz ayırım:** MCP Client (`McpAdapter`) ayrı, MCP Server (`llm_gateway_server`) ayrı | ✅ Doğru karar. McpAdapter LlProvider trait'ini implemente eder — bu sayede Gateway herhangi bir MCP tool'unu bir provider olarak görebilir. |
| **Bloklayıcı I/O tasarımı** | ✅ Async runtime gereksinimini ortadan kaldırır. MCP transport'u arka plan thread'leriyle yönetilir. |
| **JSON-RPC 2.0 standartlarına uyum** | ✅ Hata kodları, notify handling, transport katmanı eksiksiz. |
| **Stdio transport** | ✅ MCP spesifikasyonuna tam uyumlu. Gelecekte WebSocket/SSE için `Transport` trait'i hazır. |
| **Thread safety** | ✅ Tüm state `Arc<Mutex<>>` veya `AtomicBool` arkasında. Send + Sync garantili. |

### Zayıf Yönler

| Özellik | Değerlendirme |
|----------|--------------|
| **MCP Server yanlış crate'te** | ❌ `browseros-llm` bir LLM crate'idir. Tarayıcı/domain/DAG yeteneklerini bu crate'e koymak "god crate" anti-pattern'ine yol açar. |
| **Session yönetimi yok** | ❌ Mevcut server aynı anda tek client kabul eder. Session isolation, multi-tenant, credential management yok. |
| **Progress notification yok** | ❌ Uzun süren işlemlerde client geri bildirim alamaz. |
| **Sadece LLM yetenekleri** | ❌ BrowserOS'un asıl değer önerisi (browser automation) MCP üzerinden sunulamaz. |
| **Chat tool'u gereksiz** | ❌ MCP client'ların (OpenCode, Claude Code) kendi LLM'leri var. BrowserOS'un onlara LLM sağlaması anlamsız. |
| **Konfigürasyon düz metin JSON** | ❌ API key'ler için güvenli bir credential management yok. |

---

## 7. Önerilen Geçiş Stratejisi

### Phase 5C (Şimdi)
- Mevcut `llm_gateway_server.rs`'i diagnostik araç olarak işaretle
- `browseros-mcp` crate'ini oluştur (skeleton)
- McpServer ana struct'ını, Tool trait'ini ve ToolRegistry'yi tanımla
- Sadece `system::health` tool'unu implemente et (mevcut health'i taşı + session ekle)

### Phase 6
- `browseros-mcp/src/tools/llm.rs` — LlGateway entegrasyonu
- `browseros-mcp/src/tools/session.rs` — SessionManager
- `browseros-mcp/src/tools/system.rs` — Health, config, metrics
- MCP progress notification desteği

### Phase 7
- `browseros-mcp/src/tools/browser.rs` — Navigate, screenshot, evaluate
- `browseros-mcp/src/tools/dom.rs` — Query, click, type, observe
- `browseros-mcp/src/tools/network.rs` — Intercept, mock, har

### Phase 8
- `browseros-mcp/src/tools/workflow.rs` — DAG execute, plan, status
- `browseros-mcp/src/tools/storage.rs` — Cookies, localstorage
- Event subscription mekanizması
- WebSocket/SSE transport

---

*Bu doküman mevcut implementasyonu değiştirmez. Sadece mimari karar ve öneri içerir.*
