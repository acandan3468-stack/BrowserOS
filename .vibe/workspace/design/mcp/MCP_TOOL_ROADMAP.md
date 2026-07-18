# MCP Tool Roadmap — BrowserOS

**Tarih:** 2026-07-16  
**Kapsam:** Tüm BrowserOS yeteneklerinin MCP tool kategorileri, phase bazında planlama  
**Varsayım:** `browseros-mcp` crate'i oluşturuldu. Mevcut `llm_gateway_server.rs` diagnostik amaçlı korundu.

---

## 1. Tool Kategorileri

### 1.1 Session — Tarayıcı Oturum Yönetimi

BrowserOS'un kalbi. Her MCP client kendi izole BrowserOS session'ını alır.

| MCP Tool Adı | Açıklama | BrowserOS Karşılığı | Phase |
|-------------|----------|---------------------|-------|
| `session/create` | Yeni BrowserOS oturumu aç | `BrowserPool::launch()` + `Session::new()` | 6 |
| `session/close` | Oturumu kapat, kaynakları temizle | `Session::shutdown()` | 6 |
| `session/list` | Aktif oturumları listele | `BrowserPool::sessions()` | 6 |
| `session/config` | Oturum konfigürasyonunu oku/değiştir | `Session::config()` | 6 |
| `session/credentials` | Oturum için credential yönetimi | `CredentialStore::set()` | 7 |

### 1.2 Browser — Sayfa Yönetimi

Bir oturum içinde birden çok sekme/sayfa.

| MCP Tool Adı | Açıklama | BrowserOS Karşılığı | Phase |
|-------------|----------|---------------------|-------|
| `browser/navigate` | URL'ye git | `BrowserPort::navigate()` | 7 |
| `browser/go_back` | Geri git | `BrowserPort::go_back()` | 7 |
| `browser/go_forward` | İleri git | `BrowserPort::go_forward()` | 7 |
| `browser/reload` | Sayfayı yenile | `BrowserPort::reload()` | 7 |
| `browser/screenshot` | Ekran görüntüsü al | `BrowserPort::screenshot()` | 7 |
| `browser/pdf` | Sayfayı PDF'e çevir | `BrowserPort::print_to_pdf()` | 7 |
| `browser/evaluate` | JavaScript çalıştır | `BrowserPort::evaluate()` | 7 |
| `browser/title` | Sayfa başlığını al | `PageHandle::title()` | 7 |
| `browser/url` | Mevcut URL'yi al | `PageHandle::url()` | 7 |
| `browser/tabs` | Açık sekmeleri listele | `BrowserPort::pages()` | 7 |
| `browser/new_tab` | Yeni sekme aç | `BrowserPort::new_page()` | 7 |
| `browser/close_tab` | Sekmeyi kapat | `BrowserPort::close_page()` | 7 |
| `browser/wait_for_navigation` | Navigasyonun tamamlanmasını bekle | `PageHandle::wait_for_navigation()` | 7 |

### 1.3 DOM — Sayfa İçeriği Etkileşimi

Sayfa elementleriyle etkileşim için temel araçlar.

| MCP Tool Adı | Açıklama | BrowserOS Karşılığı | Phase |
|-------------|----------|---------------------|-------|
| `dom/query_selector` | CSS seçici ile element bul | `DomService::query_selector()` | 7 |
| `dom/query_selector_all` | Tüm eşleşen elementleri bul | `DomService::query_selector_all()` | 7 |
| `dom/click` | Elemente tıkla | `ElementHandle::click()` | 7 |
| `dom/type_text` | Metin yaz | `ElementHandle::type_text()` | 7 |
| `dom/get_text` | Element metnini al | `ElementHandle::text_content()` | 7 |
| `dom/get_attribute` | Element attribute'ünü al | `ElementHandle::get_attribute()` | 7 |
| `dom/set_attribute` | Element attribute'ü belirle | `ElementHandle::set_attribute()` | 7 |
| `dom/get_html` | Element inner HTML'ini al | `ElementHandle::inner_html()` | 7 |
| `dom/set_html` | Element inner HTML'ini belirle | `ElementHandle::set_inner_html()` | 7 |
| `dom/get_value` | Form element değerini al | `ElementHandle::value()` | 7 |
| `dom/set_value` | Form element değerini belirle | `ElementHandle::set_value()` | 7 |
| `dom/select` | Select elementinde seçenek seç | `ElementHandle::select_option()` | 7 |
| `dom/hover` | Elementin üzerine gel | `ElementHandle::hover()` | 7 |
| `dom/focus` | Elemente odaklan | `ElementHandle::focus()` | 7 |
| `dom/scroll_into_view` | Elemente kaydır | `ElementHandle::scroll_into_view()` | 7 |
| `dom/snapshot` | DOM ağacının snapshot'ını al | `FrameHandle::snapshot()` | 7 |
| `dom/wait_for_element` | Element görünene kadar bekle | `DomService::wait_for_selector()` | 7 |
| `dom/wait_for_function` | JS fonksiyonu true dönene kadar bekle | `PageHandle::wait_for_function()` | 7 |
| `dom/observe_mutations` | DOM değişikliklerini dinle | `MutationObserver` | 8 |
| `dom/find_by_text` | Metin içeriğine göre element bul | `DomService::find_by_text()` | 7 |
| `dom/find_by_placeholder` | Placeholder'a göre input bul | `DomService::find_by_placeholder()` | 7 |
| `dom/find_by_role` | ARIA rolüne göre element bul | `DomService::find_by_role()` | 7 |
| `dom/find_by_label` | Label metnine göre element bul | `DomService::find_by_label()` | 7 |

### 1.4 Network — Ağ Trafiği Yönetimi

Ağ isteklerini izleme, engelleme, manipüle etme.

| MCP Tool Adı | Açıklama | BrowserOS Karşılığı | Phase |
|-------------|----------|---------------------|-------|
| `network/intercept` | İstekleri yakalamaya başla | `NetworkHandle::intercept()` | 7 |
| `network/continue_request` | Yakalanan isteğe devam et | `InterceptionHandle::continue_()` | 7 |
| `network/abort_request` | Yakalanan isteği iptal et | `InterceptionHandle::abort()` | 7 |
| `network/fulfill_request` | Yakalanan isteğe mock yanıt gönder | `InterceptionHandle::fulfill()` | 7 |
| `network/set_conditions` | Ağ koşullarını simüle et | `NetworkHandle::set_conditions()` | 7 |
| `network/clear_conditions` | Ağ koşullarını sıfırla | `NetworkHandle::clear_conditions()` | 7 |
| `network/get_cookies` | Çerezleri oku | `CookieManager::list()` | 8 |
| `network/set_cookie` | Çerez belirle | `CookieManager::set()` | 8 |
| `network/delete_cookie` | Çerez sil | `CookieManager::delete()` | 8 |
| `network/clear_cookies` | Tüm çerezleri temizle | `CookieManager::clear()` | 8 |
| `network/get_har` | HAR kaydını al | `NetworkHandle::get_har()` | 8 |
| `network/start_har` | HAR kaydını başlat | `NetworkHandle::start_har()` | 8 |
| `network/stop_har` | HAR kaydını durdur | `NetworkHandle::stop_har()` | 8 |
| `network/list_requests` | Yakalanan istekleri listele | `NetworkHandle::requests()` | 7 |

### 1.5 Storage — Veri Depolama

Tarayıcı depolama alanlarına erişim.

| MCP Tool Adı | Açıklama | BrowserOS Karşılığı | Phase |
|-------------|----------|---------------------|-------|
| `storage/localstorage_get` | LocalStorage değeri oku | `StorageManager::localstorage_get()` | 8 |
| `storage/localstorage_set` | LocalStorage değeri belirle | `StorageManager::localstorage_set()` | 8 |
| `storage/localstorage_delete` | LocalStorage değeri sil | `StorageManager::localstorage_delete()` | 8 |
| `storage/localstorage_clear` | LocalStorage'ı temizle | `StorageManager::localstorage_clear()` | 8 |
| `storage/sessionstorage_get` | SessionStorage değeri oku | `StorageManager::sessionstorage_get()` | 8 |
| `storage/sessionstorage_set` | SessionStorage değeri belirle | `StorageManager::sessionstorage_set()` | 8 |
| `storage/sessionstorage_delete` | SessionStorage değeri sil | `StorageManager::sessionstorage_delete()` | 8 |
| `storage/sessionstorage_clear` | SessionStorage'ı temizle | `StorageManager::sessionstorage_clear()` | 8 |
| `storage/indexeddb_list` | IndexedDB veritabanlarını listele | `StorageManager::indexeddb_databases()` | 8 |
| `storage/indexeddb_delete` | IndexedDB veritabanını sil | `StorageManager::indexeddb_delete_database()` | 8 |

### 1.6 Workflow — Görev Otomasyonu (DAG)

Birden çok adımdan oluşan iş akışlarını planlama ve yürütme.

| MCP Tool Adı | Açıklama | BrowserOS Karşılığı | Phase |
|-------------|----------|---------------------|-------|
| `workflow/execute` | DAG workflow'u çalıştır | `DagEngine::execute()` | 8 |
| `workflow/plan` | Doğal dilden workflow planı oluştur | `DagEngine::plan()` + LLM | 8 |
| `workflow/status` | Workflow durumunu sorgula | `DagExecution::status()` | 8 |
| `workflow/cancel` | Workflow'u iptal et | `DagExecution::cancel()` | 8 |
| `workflow/list` | Workflow'ları listele | `DagEngine::executions()` | 8 |
| `workflow/result` | Workflow sonucunu al | `DagExecution::result()` | 8 |
| `workflow/define` | Yeni workflow tanımı oluştur | `DagEngine::define()` | 8 |
| `workflow/validate` | Workflow tanımını doğrula | `DagEngine::validate()` | 8 |
| `workflow/get_logs` | Workflow loglarını al | `DagExecution::logs()` | 8 |

### 1.7 LLM — Dil Modeli Erişimi (İçsel)

BrowserOS içsel LLM erişimi. **Production MCP server'da yer almaz, diagnostic içindir.**

| MCP Tool Adı | Açıklama | Phase |
|-------------|----------|-------|
| `llm/chat` | LLM sohbet (diagnostic) | 5C (mevcut) |
| `llm/embed` | Embedding (diagnostic) | 5C (mevcut) |
| `llm/models` | Kullanılabilir modelleri listele | 6 |
| `llm/completion` | Completion (isteğe bağlı) | 7 |

### 1.8 System — Sistem Yönetimi

BrowserOS MCP sunucusunun kendi yönetimi.

| MCP Tool Adı | Açıklama | BrowserOS Karşılığı | Phase |
|-------------|----------|---------------------|-------|
| `system/health` | Sunucu sağlık durumu | `RuntimeContext::health()` | 5C |
| `system/config` | Sunucu konfigürasyonu | `RuntimeContext::config()` | 6 |
| `system/metrics` | Performans metrikleri | `MetricsRegistry::snapshot()` | 6 |
| `system/logs` | Log seviyesini değiştir | `Logger::set_level()` | 6 |
| `system/version` | BrowserOS versiyon bilgisi | metadata | 5C |
| `system/shutdown` | Sunucuyu kapat | `LifecycleManager::shutdown()` | 7 |

---

## 2. Phase Bazında Tool Dağılımı

```
Phase 5C (Mevcut + yeni crate)
├── system/health       (taşı: llm_gateway_server → browseros-mcp)
├── system/version      (yeni)
└── llm/chat            (diagnostic — browseros-llm'de kalır)

Phase 6 (102 tools)
├── session/create      (yeni)
├── session/close       (yeni)
├── session/list        (yeni)
├── session/config      (yeni)
├── llm/models          (yeni — browseros-llm üzerinden)
├── system/config       (yeni)
├── system/metrics      (yeni)
├── system/logs         (yeni)
└── NOT: chat/embed tools browseros-mcp'ye taşınmaz

Phase 7 (22 tools)
├── browser/*           (12 tool)
├── dom/*               (11 tool)
├── network/*           (7 tool — interception)
└── system/shutdown

Phase 8 (18 tools)
├── network/*           (6 tool — cookies, har)
├── storage/*           (10 tool)
├── workflow/*          (9 tool)
├── dom/*               (1 tool — mutations)
└── Event subscription  (mekanizma)
```

**Toplam: 50+ MCP tool**

---

## 3. Chat Tool'unun Kaderi

| Kullanım Senaryosu | Chat Tool Gerekli mi? | Açıklama |
|-------------------|----------------------|-----------|
| OpenCode kullanıcısı Browser'a komut veriyor | ❌ Hayır | OpenCode kendi LLM'ini kullanır. BrowserOS sadece tool sağlar. |
| BrowserOS kendi kendine karar veriyor | ❌ Hayır | BrowserOS içsel olarak `browseros-llm`'i kullanır. MCP tool'u gerekmez. |
| Geliştirici LLM bağlantısını test ediyor | ✅ Diagnostic | `llm_gateway_server.exe`'de kalabilir. |
| MCP client BrowserOS'un LLM'ini kullanmak istiyor | ⚠️ İsteğe bağlı | `browseros-llm` bir MCP server değil, bir crate'tir. İsteyen client kendi MCP server'ını yazıp `browseros-llm`'e bağlanabilir. |

**Karar:** `chat` tool'u `browseros-mcp`'nin tool listesinde yer ALMAZ.  
`llm/chat` diagnostic olarak `browseros-llm` içinde kalır.

---

*Bu doküman mevcut implementasyonu değiştirmez. Sadece planlama ve öneri içerir.*
