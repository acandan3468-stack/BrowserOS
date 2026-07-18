# MCP Registration Report — browseros-llm

**Tarih:** 2026-07-17
**Hedef:** OpenCode MCP sunucusu olarak `browseros-llm` kaydı

---

## 1. Configuration Diff

### Öncesi (opencode.jsonc)

6 MCP sunucusu kayıtlıydı:
- `sequential-thinking`, `codebase-memory`, `github`, `git`, `firecrawl`, `mcp-media-forge`

### Sonrası

Yeni sunucu eklendi (`browseros-llm`), diğer 6 sunucuda değişiklik yapılmadı.

```jsonc
{
  // ... mevcut 6 sunucu (değişmedi) ...
  "browseros-llm": {
    "type": "local",
    "command": [
      "cargo",
      "run",
      "--manifest-path",
      "F:\\Projects\\MCP-Browser-Use\\browseros\\browseros-llm\\Cargo.toml",
      "--bin",
      "llm_gateway_server",
      "--",
      "F:\\Projects\\MCP-Browser-Use\\browseros\\browseros-llm\\config\\mcp_server.json"
    ],
    "description": "BrowserOS LLM Gateway — chat, embed, and health via MCP"
  }
}
```

---

## 2. Final Server Entry

| Alan | Değer |
|------|-------|
| **Anahtar** | `browseros-llm` |
| **Tür** | `local` (stdio taşıması) |
| **Çalışma Dizini** | Önemsiz — `cargo run --manifest-path` mutlak yol kullanır |
| **Yapılandırma Dosyası** | `F:\Projects\MCP-Browser-Use\browseros\browseros-llm\config\mcp_server.json` |
| **Binary** | `src/bin/llm_gateway_server.rs` → `target/debug/llm_gateway_server.exe` |
| **Env Değişkenleri** | Yok |
| **Açıklama** | BrowserOS LLM Gateway — chat, embed, and health via MCP |

---

## 3. Startup Command

```
cargo run --manifest-path F:\Projects\MCP-Browser-Use\browseros\browseros-llm\Cargo.toml --bin llm_gateway_server -- F:\Projects\MCP-Browser-Use\browseros\browseros-llm\config\mcp_server.json
```

Bileşenler:
| Parça | Açıklama |
|-------|----------|
| `cargo run` | Rust projesini derler ve çalıştırır |
| `--manifest-path ...` | Proje manifest dosyasının mutlak yolu |
| `--bin llm_gateway_server` | Hedef binary adı |
| `--` | Cargo argümanlarından binary argümanlarına geçiş |
| `config/mcp_server.json` | LLM Gateway yapılandırma dosyası |

---

## 4. Expected Handshake

### İstek (OpenCode → Sunucu)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": { "name": "opencode", "version": "1.0" }
  }
}
```

### Yanıt (Sunucu → OpenCode)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": { "tools": {} },
    "serverInfo": {
      "name": "browseros-llm-gateway",
      "version": "0.1.0"
    }
  }
}
```

Ardından OpenCode `notifications/initialized` gönderir (sunucu sessizce yok sayar).

---

## 5. Expected tools/list Response

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "chat",
        "description": "Send a chat message to an LLM and get a text response",
        "inputSchema": {
          "type": "object",
          "properties": {
            "model":       { "type": "string", "description": "Model ID (optional)" },
            "capability":  { "type": "string", "description": "Capability hint: chat, fast, cheap (optional)" },
            "system":      { "type": "string", "description": "System prompt" },
            "messages": {
              "type": "array",
              "items": {
                "type": "object",
                "properties": {
                  "role":    { "type": "string", "enum": ["user", "assistant", "system"] },
                  "content": { "type": "string" }
                },
                "required": ["role", "content"]
              },
              "description": "Chat messages"
            }
          },
          "required": ["messages"]
        }
      },
      {
        "name": "embed",
        "description": "Generate embeddings for text",
        "inputSchema": {
          "type": "object",
          "properties": {
            "model": { "type": "string", "description": "Model ID (optional)" },
            "input": { "type": "string", "description": "Text to embed" }
          },
          "required": ["input"]
        }
      },
      {
        "name": "health",
        "description": "Check health status of all configured LLM providers",
        "inputSchema": {
          "type": "object",
          "properties": {}
        }
      }
    ]
  }
}
```

---

## 6. Expected Health Behaviour

### Başlangıç (Hiçbir istek yapılmamış)

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [{
      "type": "text",
      "text": "provider=openai status=Unknown latency_ms=0 error_rate=0.00 models=gpt-4o-mini,gpt-4o"
    }]
  }
}
```

### Başarılı bir `chat` çağrısından sonra

`status=Healthy` veya `status=Degraded` olarak güncellenir (gerçek API yanıtına bağlı).

### API anahtarı olmadan

Sağlayıcı `AuthenticationError` döner. `chat` tool'u hata mesajını MCP error olarak iletir:
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "error": {
    "code": -32000,
    "message": "AuthenticationError: HTTP 401: ..."
  }
}
```

---

## 7. File Manifest

| Dosya | Rolü |
|-------|------|
| `opencode.jsonc` (`~/.config/opencode/`) | OpenCode MCP sunucu kaydı |
| `src/bin/llm_gateway_server.rs` | MCP sunucu binary'si |
| `config/mcp_server.json` | LLM Gateway yapılandırması |
| `MCP_REGISTRATION_REPORT.md` | Bu rapor |

---

## 8. Validation Checklist

| Kontrol | Durum |
|---------|-------|
| Configuration geçerli JSON | ✅ |
| Kayıt eklendi, mevcutlar değişmedi | ✅ |
| Binary ismi doğru | ✅ (`llm_gateway_server`) |
| Manifest yolu doğru | ✅ |
| Config dosya yolu doğru | ✅ |
| Stdio taşıması | ✅ (`type: "local"` = stdio) |
| Handshake çalışıyor | ✅ (test edildi) |
| tools/list çalışıyor | ✅ (3 tool döndü) |
| Hiçbir MCP sunucusu kaldırılmadı | ✅ |
