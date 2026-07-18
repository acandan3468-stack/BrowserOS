# MCP API Surface — BrowserOS

**Tarih:** 2026-07-16  
**Kapsam:** BrowserOS MCP sunucusunun nihai API yüzey tasarımı. Her tool için input schema, output format, hata durumları.  
**Varsayım:** `browseros-mcp` crate'i oluşturuldu. Tool'lar `McpServer` tarafından yönetiliyor.

---

## 1. Genel Prensipler

| Prensip | Açıklama |
|----------|-----------|
| **JSON-RPC 2.0** | MCP spesifikasyonuna tam uyum |
| **Session-scoped** | Her tool çağrısı bir `session_id` parametresi alır (opsiyonel — default session kullanılır) |
| **Progress notifications** | >5sn sürecek işlemler `notifications/progress` gönderir (0.0–1.0) |
| **Error codes** | `-32000` application error (hata mesajı ile), `-32001` session not found, `-32002` browser error, `-32003` timeout |
| **Idempotency** | Sadece idempotent toollar (get, list, snapshot) `idempotency_key` parametresi alır |
| **Pagination** | Liste döndüren toollar `cursor` ve `limit` parametreleri alır |
| **Rate limiting** | 429 hatası dönmez — BrowserOS kendi içinde kuyruklar |

---

## 2. Tool Tanımları

### 2.1 System Tools

#### `system/health`

```json
{
  "name": "system/health",
  "description": "Check health status of all BrowserOS subsystems",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string", "description": "Optional session ID" }
    }
  },
  "output": {
    "status": "ok|degraded|error",
    "version": "0.1.0",
    "uptime_secs": 3600,
    "subsystems": {
      "browser": { "status": "ok", "active_sessions": 2 },
      "llm": { "status": "ok", "provider_count": 3 },
      "dag": { "status": "ok", "running_executions": 1 },
      "network": { "status": "degraded", "message": "No CDP connection" }
    }
  }
}
```

#### `system/config`

```json
{
  "name": "system/config",
  "description": "Read or update BrowserOS configuration",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "description": "Config key path (e.g. 'llm.default_provider')" },
      "value": { "description": "New value to set (omit to read)" }
    }
  },
  "output": {
    "path": "llm.default_provider",
    "value": "openai",
    "updated": false
  }
}
```

#### `system/metrics`

```json
{
  "name": "system/metrics",
  "description": "Get runtime performance metrics",
  "inputSchema": {
    "type": "object",
    "properties": {
      "names": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Metric names to filter (empty = all)"
      }
    }
  },
  "output": {
    "metrics": {
      "llm.requests.total": { "count": 150, "rate_1m": 2.3 },
      "browser.navigations": { "count": 42, "rate_1m": 0.5 },
      "dom.queries": { "count": 310, "rate_1m": 5.1 },
      "workflow.executions": { "count": 8, "rate_1m": 0.1 }
    },
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

#### `system/logs`

```json
{
  "name": "system/logs",
  "description": "Set log level or retrieve recent logs",
  "inputSchema": {
    "type": "object",
    "properties": {
      "level": { "type": "string", "enum": ["trace", "debug", "info", "warn", "error"], "description": "Set log level" },
      "tail": { "type": "integer", "description": "Number of recent log lines to return" }
    }
  },
  "output": {
    "current_level": "info",
    "logs": [
      { "timestamp": "...", "level": "info", "target": "browseros-mcp", "message": "Session created" }
    ]
  }
}
```

#### `system/version`

```json
{
  "name": "system/version",
  "description": "Get BrowserOS version information",
  "inputSchema": { "type": "object", "properties": {} },
  "output": {
    "version": "0.5.0",
    "codename": "Ferris",
    "commit": "abc123def",
    "built": "2026-07-16T10:00:00Z",
    "rust_version": "1.85.0",
    "features": ["llm", "browser", "dom", "dag", "network"]
  }
}
```

---

### 2.2 Session Tools

#### `session/create`

```json
{
  "name": "session/create",
  "description": "Create a new isolated BrowserOS session",
  "inputSchema": {
    "type": "object",
    "properties": {
      "browser_type": { "type": "string", "enum": ["chromium", "firefox"], "default": "chromium" },
      "headless": { "type": "boolean", "default": true },
      "viewport": {
        "type": "object",
        "properties": {
          "width": { "type": "integer", "default": 1280 },
          "height": { "type": "integer", "default": 720 }
        }
      },
      "locale": { "type": "string", "default": "en-US" },
      "timezone": { "type": "string", "description": "IANA timezone ID" },
      "geolocation": {
        "type": "object",
        "properties": {
          "latitude": { "type": "number" },
          "longitude": { "type": "number" }
        }
      },
      "proxy": { "type": "object", "properties": {
        "server": { "type": "string" },
        "username": { "type": "string" },
        "password": { "type": "string" }
      }},
      "timeout_ms": { "type": "integer", "default": 30000 }
    }
  },
  "output": {
    "session_id": "sess_abc123",
    "browser": { "type": "chromium", "version": "130.0", "pid": 12345 },
    "created_at": "2026-07-16T12:00:00Z"
  }
}
```

#### `session/close`

```json
{
  "name": "session/close",
  "description": "Close a BrowserOS session and release all resources",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string", "description": "Session ID to close" }
    },
    "required": ["session_id"]
  },
  "output": {
    "session_id": "sess_abc123",
    "status": "closed",
    "duration_secs": 300,
    "resource_summary": {
      "navigations": 15,
      "dom_queries": 120,
      "network_requests": 450,
      "workflow_executions": 2
    }
  }
}
```

---

### 2.3 Browser Tools

#### `browser/navigate`

```json
{
  "name": "browser/navigate",
  "description": "Navigate to a URL in the current page",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "url": { "type": "string", "format": "uri" },
      "referer": { "type": "string" },
      "wait_until": { "type": "string", "enum": ["load", "domcontentloaded", "networkidle"], "default": "load" },
      "timeout_ms": { "type": "integer", "default": 30000 }
    },
    "required": ["url"]
  },
  "output": {
    "url": "https://example.com",
    "title": "Example Domain",
    "status_code": 200,
    "navigation_id": "nav_001"
  }
}
```

#### `browser/screenshot`

```json
{
  "name": "browser/screenshot",
  "description": "Capture a screenshot of the current page",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "full_page": { "type": "boolean", "default": false },
      "format": { "type": "string", "enum": ["png", "jpeg"], "default": "png" },
      "quality": { "type": "integer", "minimum": 0, "maximum": 100 },
      "selector": { "type": "string", "description": "CSS selector to capture only a specific element" }
    }
  },
  "output": {
    "data": "<base64-encoded image>",
    "mime_type": "image/png",
    "width": 1280,
    "height": 720
  }
}
```

#### `browser/evaluate`

```json
{
  "name": "browser/evaluate",
  "description": "Execute JavaScript in the page context",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "expression": { "type": "string" },
      "await_promise": { "type": "boolean", "default": true },
      "timeout_ms": { "type": "integer", "default": 10000 }
    },
    "required": ["expression"]
  },
  "output": {
    "result": "<JSON-serialized result>",
    "type": "string|number|boolean|object|array|null",
    "has_error": false
  }
}
```

---

### 2.4 DOM Tools

#### `dom/query_selector`

```json
{
  "name": "dom/query_selector",
  "description": "Find first element matching CSS selector",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "selector": { "type": "string", "description": "CSS selector" },
      "timeout_ms": { "type": "integer", "default": 5000 }
    },
    "required": ["selector"]
  },
  "output": {
    "element_id": "elem_001",
    "tag": "button",
    "attributes": {
      "id": "submit-btn",
      "class": "btn primary",
      "type": "submit"
    },
    "text": "Submit",
    "is_visible": true,
    "bounding_box": { "x": 100, "y": 200, "width": 120, "height": 40 }
  }
}
```

#### `dom/click`

```json
{
  "name": "dom/click",
  "description": "Click an element identified by element_id or selector",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "element_id": { "type": "string", "description": "Element ID from query_selector" },
      "selector": { "type": "string", "description": "CSS selector (alternative to element_id)" },
      "button": { "type": "string", "enum": ["left", "middle", "right"], "default": "left" },
      "click_count": { "type": "integer", "default": 1 },
      "delay_ms": { "type": "integer", "default": 0 },
      "timeout_ms": { "type": "integer", "default": 5000 }
    },
    "oneOf": [
      { "required": ["element_id"] },
      { "required": ["selector"] }
    ]
  },
  "output": {
    "element_id": "elem_001",
    "url": "https://example.com/new-page",
    "navigation_occurred": true
  }
}
```

#### `dom/type_text`

```json
{
  "name": "dom/type_text",
  "description": "Type text into an input element",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "element_id": { "type": "string" },
      "selector": { "type": "string" },
      "text": { "type": "string" },
      "clear_first": { "type": "boolean", "default": true },
      "delay_ms": { "type": "integer", "default": 10 },
      "timeout_ms": { "type": "integer", "default": 5000 }
    },
    "oneOf": [
      { "required": ["element_id", "text"] },
      { "required": ["selector", "text"] }
    ]
  },
  "output": {
    "element_id": "elem_002",
    "value": "typed text"
  }
}
```

#### `dom/snapshot`

```json
{
  "name": "dom/snapshot",
  "description": "Get a snapshot of the current DOM tree",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "max_depth": { "type": "integer", "default": 5 },
      "selector_filter": { "type": "string", "description": "CSS selector to filter which elements to include" },
      "include_computed_styles": { "type": "boolean", "default": false },
      "include_attributes": { "type": "boolean", "default": true },
      "include_text": { "type": "boolean", "default": true }
    }
  },
  "output": {
    "nodes": [
      {
        "element_id": "elem_000",
        "tag": "html",
        "children": [
          { "element_id": "elem_001", "tag": "head" },
          { "element_id": "elem_002", "tag": "body", "children": [
            { "element_id": "elem_003", "tag": "h1", "text": "Hello" }
          ]}
        ]
      }
    ],
    "node_count": 42,
    "truncated": false
  }
}
```

---

### 2.5 Network Tools

#### `network/intercept`

```json
{
  "name": "network/intercept",
  "description": "Start intercepting network requests matching a pattern",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "url_pattern": { "type": "string", "description": "URL glob pattern (e.g. *api/*)" },
      "resource_types": { "type": "array", "items": { "type": "string", "enum": ["document", "script", "stylesheet", "image", "font", "xhr", "fetch", "websocket"] } },
      "action": { "type": "string", "enum": ["block", "allow", "mock"] }
    },
    "required": ["url_pattern"]
  },
  "output": {
    "interception_id": "int_001",
    "url_pattern": "*api/*",
    "active": true
  }
}
```

#### `network/set_conditions`

```json
{
  "name": "network/set_conditions",
  "description": "Simulate network conditions",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "offline": { "type": "boolean" },
      "latency_ms": { "type": "integer" },
      "download_throughput_kbps": { "type": "integer" },
      "upload_throughput_kbps": { "type": "integer" },
      "connection_type": { "type": "string", "enum": ["wifi", "cellular3g", "cellular4g", "offline"] }
    }
  },
  "output": {
    "applied": true,
    "conditions": {
      "offline": false,
      "latency_ms": 150,
      "download_throughput_kbps": 5000
    }
  }
}
```

---

### 2.6 Workflow Tools

#### `workflow/execute`

```json
{
  "name": "workflow/execute",
  "description": "Execute a DAG workflow with given steps",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "steps": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "id": { "type": "string" },
            "tool": { "type": "string" },
            "args": { "type": "object" },
            "depends_on": { "type": "array", "items": { "type": "string" } }
          },
          "required": ["id", "tool", "args"]
        }
      },
      "max_concurrency": { "type": "integer", "default": 3 }
    },
    "required": ["steps"]
  },
  "output": {
    "execution_id": "wf_001",
    "status": "running",
    "total_steps": 5,
    "progress": 0.4
  }
}
```

#### `workflow/plan`

```json
{
  "name": "workflow/plan",
  "description": "Generate a workflow plan from natural language using LLM",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "goal": { "type": "string", "description": "Natural language description of what to do" },
      "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Restrict to these tools" }
    },
    "required": ["goal"]
  },
  "output": {
    "goal": "Log in to example.com and extract the dashboard data",
    "steps": [
      { "id": "1", "tool": "browser/navigate", "args": { "url": "https://example.com/login" }, "depends_on": [] },
      { "id": "2", "tool": "dom/type_text", "args": { "selector": "#username", "text": "..." }, "depends_on": ["1"] },
      { "id": "3", "tool": "dom/type_text", "args": { "selector": "#password", "text": "..." }, "depends_on": ["1"] },
      { "id": "4", "tool": "dom/click", "args": { "selector": "#login-btn" }, "depends_on": ["2", "3"] },
      { "id": "5", "tool": "dom/snapshot", "args": {}, "depends_on": ["4"] }
    ],
    "confidence": 0.85,
    "warnings": ["Password field detected — consider using credentials manager"]
  }
}
```

---

### 2.7 Storage Tools

#### `storage/cookies_get`

```json
{
  "name": "storage/cookies_get",
  "description": "Get cookies for a URL",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": { "type": "string" },
      "urls": { "type": "array", "items": { "type": "string" }, "description": "URLs to get cookies for" }
    }
  },
  "output": {
    "cookies": [
      { "name": "session", "value": "abc123", "domain": ".example.com", "path": "/", "secure": true, "http_only": true }
    ]
  }
}
```

---

## 3. Hata Kodları

| Kod | Anlam | Örnek |
|-----|-------|-------|
| `-32700` | Parse error | Geçersiz JSON |
| `-32600` | Invalid request | Eksik `method` alanı |
| `-32601` | Method not found | Bilinmeyen tool adı |
| `-32602` | Invalid params | Eksik zorunlu parametre |
| `-32603` | Internal error | Beklenmeyen hata |
| `-32000` | Application error | Genel hata mesajı |
| `-32001` | Session not found | Geçersiz session_id |
| `-32002` | Browser error | Tarayıcı çöktü, bağlantı koptu |
| `-32003` | Timeout | İşlem timeout'a düştü |
| `-32004` | Element not found | DOM'da element bulunamadı |
| `-32005` | Navigation failed | Sayfa yüklenemedi |
| `-32006` | Workflow error | Workflow yürütme hatası |
| `-32007` | LLM error | LLM provider hatası |

---

## 4. Event / Notification Mekanizması

MCP spesifikasyonundaki `notifications/` prefix'i ile BrowserOS olayları dışa açılır.

| Notification | Tetikleyici | Payload |
|-------------|-------------|---------|
| `notifications/progress` | Uzun süren işlem | `{ execution_id, progress: 0.0–1.0, message }` |
| `notifications/browser/navigated` | Sayfa yüklendi | `{ url, title, status_code }` |
| `notifications/browser/console` | console.log mesajı | `{ level, text, location }` |
| `notifications/browser/dialog` | alert/confirm/prompt | `{ type, message, default_value }` |
| `notifications/dom/mutation` | DOM değişikliği | `{ added: [], removed: [], attribute_changed: [] }` |
| `notifications/network/request` | HTTP isteği | `{ url, method, headers }` |
| `notifications/network/response` | HTTP yanıtı | `{ url, status_code, headers }` |
| `notifications/workflow/step_completed` | Workflow adımı bitti | `{ execution_id, step_id, result }` |
| `notifications/workflow/completed` | Workflow tamamlandı | `{ execution_id, status, summary }` |

---

*Bu doküman mevcut implementasyonu değiştirmez. Sadece API tasarım önerisi içerir.*
