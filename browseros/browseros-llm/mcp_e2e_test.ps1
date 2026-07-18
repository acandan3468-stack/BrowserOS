# MCP End-to-End Validation Script
# Tests browseros-llm MCP server through real stdio transport

$ConfigPath = "F:\Projects\MCP-Browser-Use\browseros\browseros-llm\config\mcp_server.json"
$ServerBinary = "F:\Projects\MCP-Browser-Use\browseros\browseros-llm\target\debug\llm_gateway_server.exe"
$Results = @()

function Send-McpMessage($Process, $Message, $Label) {
    $json = $Message | ConvertTo-Json -Depth 10 -Compress
    $start = [System.Diagnostics.Stopwatch]::StartNew()
    $Process.StandardInput.WriteLine($json)
    $Process.StandardInput.Flush()
    $response = $Process.StandardOutput.ReadLine()
    $start.Stop()
    
    $result = @{
        Label = $Label
        Request = $json
        Response = $response
        LatencyMs = $start.ElapsedMilliseconds
        Success = $null -ne $response
    }
    
    if ($response) {
        try {
            $parsed = $response | ConvertFrom-Json
            if ($parsed.error) {
                $result.Success = $false
                $result.Error = $parsed.error.message
                $result.ErrorCode = $parsed.error.code
            } else {
                $result.Success = $true
            }
        } catch {
            $result.Success = $false
            $result.Error = "Failed to parse response: $_"
        }
    }
    
    $global:Results += $result
    return $result
}

function Write-Result($Result) {
    $status = if ($Result.Success) { "PASS" } else { "FAIL" }
    Write-Host "[$status] $($Result.Label)" -ForegroundColor $(if ($Result.Success) { "Green" } else { "Red" })
    Write-Host "  Latency: $($Result.LatencyMs)ms"
    if (-not $Result.Success -and $Result.Error) {
        Write-Host "  Error: $($Result.Error)" -ForegroundColor Red
    }
}

function Start-Server {
    Write-Host "`n=== Starting MCP Server ===" -ForegroundColor Cyan
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $ServerBinary
    $psi.Arguments = "`"$ConfigPath`""
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    
    $proc = New-Object System.Diagnostics.Process
    $proc.StartInfo = $psi
    $proc.Start() | Out-Null
    
    # Wait for server to be ready
    Start-Sleep -Milliseconds 500
    return $proc
}

function Stop-Server($Process) {
    if ($Process -and !$Process.HasExited) {
        Write-Host "`n=== Stopping Server ===" -ForegroundColor Cyan
        $Process.StandardInput.Close()
        $Process.WaitForExit(3000) | Out-Null
        if (!$Process.HasExited) {
            $Process.Kill()
        }
        Write-Host "Server exited with code: $($Process.ExitCode)"
    }
}

# =================================================
# SCENARIO 1: Protocol Basics (Initialize + tools/list)
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "SCENARIO 1: Protocol Basics" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

$proc = Start-Server

# 1a: Initialize
$init = @{
    jsonrpc = "2.0"
    id = 1
    method = "initialize"
    params = @{
        protocolVersion = "2024-11-05"
        capabilities = @{}
        clientInfo = @{ name = "e2e-test"; version = "1.0" }
    }
}
$r = Send-McpMessage $proc $init "S1a: initialize"
Write-Result $r

# 1b: tools/list
$list = @{
    jsonrpc = "2.0"
    id = 2
    method = "tools/list"
    params = @{}
}
$r = Send-McpMessage $proc $list "S1b: tools/list"
Write-Result $r

# 1c: notifications/initialized (should be silently ignored)
$notify = @{
    jsonrpc = "2.0"
    method = "notifications/initialized"
    params = @{}
}
$json = $notify | ConvertTo-Json -Depth 10 -Compress
$proc.StandardInput.WriteLine($json)
$proc.StandardInput.Flush()
# No response expected for notifications — check no output comes within 500ms
$hasResponse = $proc.StandardOutput.Peek() -ge 0
if (-not $hasResponse) {
    Write-Host "[PASS] S1c: notifications/initialized (no response = silent ignore)" -ForegroundColor Green
} else {
    $line = $proc.StandardOutput.ReadLine()
    Write-Host "[FAIL] S1c: notifications/initialized produced unexpected response: $line" -ForegroundColor Red
}

# =================================================
# SCENARIO 2: health tool
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "SCENARIO 2: health tool" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

# 2a: health (basic)
$health = @{
    jsonrpc = "2.0"
    id = 3
    method = "tools/call"
    params = @{
        name = "health"
        arguments = @{}
    }
}
$r = Send-McpMessage $proc $health "S2a: health (basic)"
Write-Result $r

# 2b: health (repeated sequential)
for ($i = 0; $i -lt 3; $i++) {
    $health.id = 4 + $i
    $r = Send-McpMessage $proc $health "S2b: health (seq#$($i+1))"
    Write-Result $r
}

# =================================================
# SCENARIO 3: chat tool
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "SCENARIO 3: chat tool" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

# 3a: chat with valid params (will fail with auth — that's expected)
$chat1 = @{
    jsonrpc = "2.0"
    id = 10
    method = "tools/call"
    params = @{
        name = "chat"
        arguments = @{
            messages = @(@{ role = "user"; content = "Hello!" })
        }
    }
}
$r = Send-McpMessage $proc $chat1 "S3a: chat (basic, missing API key → auth error)"
Write-Result $r

# 3b: chat with model parameter
$chat2 = @{
    jsonrpc = "2.0"
    id = 11
    method = "tools/call"
    params = @{
        name = "chat"
        arguments = @{
            model = "gpt-4o-mini"
            messages = @(@{ role = "user"; content = "Hi" })
        }
    }
}
$r = Send-McpMessage $proc $chat2 "S3b: chat (with model)"
Write-Result $r

# 3c: chat with system prompt
$chat3 = @{
    jsonrpc = "2.0"
    id = 12
    method = "tools/call"
    params = @{
        name = "chat"
        arguments = @{
            system = "You are a helpful assistant."
            messages = @(@{ role = "user"; content = "Tell me a joke" })
        }
    }
}
$r = Send-McpMessage $proc $chat3 "S3c: chat (with system prompt)"
Write-Result $r

# 3d: chat with multi-turn conversation
$chat4 = @{
    jsonrpc = "2.0"
    id = 13
    method = "tools/call"
    params = @{
        name = "chat"
        arguments = @{
            model = "gpt-4o-mini"
            messages = @(
                @{ role = "user"; content = "What is 2+2?" }
                @{ role = "assistant"; content = "4" }
                @{ role = "user"; content = "Multiply that by 3" }
            )
        }
    }
}
$r = Send-McpMessage $proc $chat4 "S3d: chat (multi-turn)"
Write-Result $r

# 3e: chat with capability hint
$chat5 = @{
    jsonrpc = "2.0"
    id = 14
    method = "tools/call"
    params = @{
        name = "chat"
        arguments = @{
            capability = "fast"
            messages = @(@{ role = "user"; content = "Quick question" })
        }
    }
}
$r = Send-McpMessage $proc $chat5 "S3e: chat (with capability)"
Write-Result $r

# 3f: chat with empty messages (edge case — missing required)
$chat6 = @{
    jsonrpc = "2.0"
    id = 15
    method = "tools/call"
    params = @{
        name = "chat"
        arguments = @{
            messages = @()
        }
    }
}
$r = Send-McpMessage $proc $chat6 "S3f: chat (empty messages array)"
Write-Result $r

# =================================================
# SCENARIO 4: embed tool
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "SCENARIO 4: embed tool" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

# 4a: embed basic
$embed1 = @{
    jsonrpc = "2.0"
    id = 20
    method = "tools/call"
    params = @{
        name = "embed"
        arguments = @{
            input = "Hello world"
        }
    }
}
$r = Send-McpMessage $proc $embed1 "S4a: embed (basic, missing API key → auth error)"
Write-Result $r

# 4b: embed with model
$embed2 = @{
    jsonrpc = "2.0"
    id = 21
    method = "tools/call"
    params = @{
        name = "embed"
        arguments = @{
            model = "gpt-4o-mini"
            input = "Embed this text"
        }
    }
}
$r = Send-McpMessage $proc $embed2 "S4b: embed (with model)"
Write-Result $r

# 4c: embed with empty input (edge case)
$embed3 = @{
    jsonrpc = "2.0"
    id = 22
    method = "tools/call"
    params = @{
        name = "embed"
        arguments = @{
            input = ""
        }
    }
}
$r = Send-McpMessage $proc $embed3 "S4c: embed (empty input)"
Write-Result $r

# =================================================
# SCENARIO 5: Error handling
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "SCENARIO 5: Error handling" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

# 5a: Malformed JSON
Write-Host "`n--- S5a: Malformed JSON ---" -ForegroundColor Cyan
$proc.StandardInput.WriteLine('{"jsonrpc": "2.0", "id": 30,')
$proc.StandardInput.Flush()
$resp = $proc.StandardOutput.ReadLine()
$start5a = [System.Diagnostics.Stopwatch]::StartNew()
$timeout5a = $false
while ($null -eq $resp -and $start5a.Elapsed.TotalSeconds -lt 5) {
    Start-Sleep -Milliseconds 100
    if ($proc.StandardOutput.Peek() -ge 0) {
        $resp = $proc.StandardOutput.ReadLine()
    }
}
if ($resp) {
    Write-Host "[PASS] S5a: malformed JSON → responded" -ForegroundColor Green
    Write-Host "  Response: $resp"
    try {
        $p = $resp | ConvertFrom-Json
        if ($p.error.code -eq -32700) { Write-Host "  Correct error code: -32700 (Parse error)" -ForegroundColor Green }
        else { Write-Host "  WARNING: unexpected error code: $($p.error.code)" -ForegroundColor Yellow }
    } catch { Write-Host "  WARNING: could not parse error response: $_" -ForegroundColor Yellow }
} else {
    Write-Host "[FAIL] S5a: malformed JSON → no response (timeout)" -ForegroundColor Red
}

# 5b: Unknown method
$unknown = @{
    jsonrpc = "2.0"
    id = 31
    method = "tools/get"
    params = @{}
}
$r = Send-McpMessage $proc $unknown "S5b: unknown method 'tools/get'"
Write-Result $r

# 5c: Unknown tool
$unk_tool = @{
    jsonrpc = "2.0"
    id = 32
    method = "tools/call"
    params = @{
        name = "nonexistent-tool"
        arguments = @{}
    }
}
$r = Send-McpMessage $proc $unk_tool "S5c: unknown tool 'nonexistent-tool'"
Write-Result $r

# 5d: Missing required params (no messages in chat)
$missing = @{
    jsonrpc = "2.0"
    id = 33
    method = "tools/call"
    params = @{
        name = "chat"
        arguments = @{}
    }
}
$r = Send-McpMessage $proc $missing "S5d: chat (missing required 'messages')"
Write-Result $r

# 5e: Unknown model (should still fail with auth, but check graceful handling)
$bad_model = @{
    jsonrpc = "2.0"
    id = 34
    method = "tools/call"
    params = @{
        name = "chat"
        arguments = @{
            model = "nonexistent-model-9000"
            messages = @(@{ role = "user"; content = "Hi" })
        }
    }
}
$r = Send-McpMessage $proc $bad_model "S5e: chat (nonexistent model)"
Write-Result $r

# =================================================
# SCENARIO 6: Sequential calls
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "SCENARIO 6: Sequential calls" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

# 6a: Sequential health calls
Write-Host "`n--- S6a: 10 sequential health calls ---" -ForegroundColor Cyan
$seqTimes = @()
for ($i = 0; $i -lt 10; $i++) {
    $health.id = 40 + $i
    $r = Send-McpMessage $proc $health "S6a: health seq#$($i+1)"
    $seqTimes += $r.LatencyMs
    Write-Result $r
}
$avgTime = ($seqTimes | Measure-Object -Average).Average
$maxTime = ($seqTimes | Measure-Object -Maximum).Maximum
$minTime = ($seqTimes | Measure-Object -Minimum).Minimum
Write-Host "  Seq stats: min=${minTime}ms avg=${avgTime:N1}ms max=${maxTime}ms" -ForegroundColor Cyan

# =================================================
# SCENARIO 7: Server shutdown
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "SCENARIO 7: Server shutdown" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

# Read any remaining stderr
$stderr = $proc.StandardError.ReadToEnd()
if ($stderr) {
    Write-Host "Server stderr during session:" -ForegroundColor Cyan
    Write-Host $stderr -ForegroundColor Gray
}

Stop-Server $proc

# =================================================
# SCENARIO 8: Reconnect
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "SCENARIO 8: Reconnect" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

$proc2 = Start-Server

$r = Send-McpMessage $proc2 $init "S8a: reconnect → initialize"
Write-Result $r

$r = Send-McpMessage $proc2 $health "S8b: reconnect → health"
Write-Result $r

Stop-Server $proc2

# =================================================
# RESULTS SUMMARY
# =================================================
Write-Host "`n========================================" -ForegroundColor Yellow
Write-Host "RESULTS SUMMARY" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow

$total = $Results.Count
$passed = ($Results | Where-Object { $_.Success }).Count
$failed = ($Results | Where-Object { -not $_.Success }).Count

Write-Host "Total: $total | Passed: $passed | Failed: $failed" -ForegroundColor $(if ($failed -eq 0) { "Green" } else { "Red" })
Write-Host "Pass rate: $(($passed/$total*100).ToString('F1'))%"

$Results | Export-Csv -Path "F:\Projects\MCP-Browser-Use\browseros\browseros-llm\mcp_e2e_results.csv" -NoTypeInformation
Write-Host "Results exported to mcp_e2e_results.csv"

if ($failed -gt 0) {
    Write-Host "`n--- FAILURES ---" -ForegroundColor Red
    $Results | Where-Object { -not $_.Success } | ForEach-Object {
        Write-Host "FAIL: $($_.Label)" -ForegroundColor Red
        Write-Host "  Error: $($_.Error)"
        Write-Host "  Latency: $($_.LatencyMs)ms"
    }
}
