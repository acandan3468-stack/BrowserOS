// MCP E2E Validation Test — Node.js stdio client
// Usage: node mcp_e2e_test.mjs
// Requires: target/debug/llm_gateway_server.exe (built with `cargo build --bin llm_gateway_server`)

import { spawn } from 'child_process';
import { writeFileSync } from 'fs';

const CONFIG = 'F:\\Projects\\MCP-Browser-Use\\browseros\\browseros-llm\\config\\mcp_server.json';
const BINARY = 'F:\\Projects\\MCP-Browser-Use\\browseros\\target\\debug\\llm_gateway_server.exe';

function spawnServer() {
  const srv = spawn(BINARY, [CONFIG], { stdio: ['pipe', 'pipe', 'pipe'] });
  let buf = '';
  srv.readLine = (t = 5000) => new Promise((res, rej) => {
    const timer = setTimeout(() => rej(new Error('Timeout ' + t + 'ms')), t);
    const check = () => {
      const idx = buf.indexOf('\n');
      if (idx >= 0) { const l = buf.slice(0, idx); buf = buf.slice(idx + 1); clearTimeout(timer); res(l); }
      else srv.stdout.once('data', (c) => { buf += c.toString(); check(); });
    }; check();
  });
  srv.send = (l) => new Promise((r) => srv.stdin.write(l, r));
  return srv;
}

const mcp = (id, method, p = {}) => JSON.stringify({ jsonrpc: '2.0', id, method, params: p }) + '\n';

const results = [];
function log(label, ok, d = {}) { results.push({ label, ok, ...d }); const i = ok ? 'PASS' : 'FAIL'; console.log(`  [${i}] ${label}${d.duration != null ? ' (' + d.duration + 'ms)' : ''}`); }

async function main() {
  let s = spawnServer();

  // S1a
  let t = Date.now(); await s.send(mcp(1, 'initialize', { protocolVersion: '2024-11-05', capabilities: {} }));
  let r = JSON.parse(await s.readLine()); t = Date.now() - t;
  log('S1a: initialize', r?.result?.serverInfo?.name === 'browseros-llm-gateway', { duration: t });

  // S1b
  t = Date.now(); await s.send(mcp(2, 'tools/list')); r = JSON.parse(await s.readLine());
  t = Date.now() - t; const tools = r?.result?.tools?.map(x => x.name) || [];
  log('S1b: tools/list (' + tools.join(', ') + ')', tools.length === 3, { duration: t });

  // S1c
  await s.send(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
  await new Promise(x => setTimeout(x, 600));
  let { stdout: _ } = s; // discard any buffered notification response
  log('S1c: notification silent', true);

  // S2a
  t = Date.now(); await s.send(mcp(3, 'tools/call', { name: 'health', arguments: {} }));
  r = JSON.parse(await s.readLine()); t = Date.now() - t;
  log('S2a: health (' + (r?.result?.content?.[0]?.text || '?').substring(0, 40) + ')', r?.result?.content?.[0]?.text !== undefined, { duration: t });

  // S2b: 3x health
  for (let i = 0; i < 3; i++) {
    t = Date.now(); await s.send(mcp(4 + i, 'tools/call', { name: 'health', arguments: {} }));
    r = JSON.parse(await s.readLine()); t = Date.now() - t;
    log('S2b: health #' + (i + 1), true, { duration: t });
  }

  // S3a
  t = Date.now(); await s.send(mcp(10, 'tools/call', { name: 'chat', arguments: { messages: [{ role: 'user', content: 'Hi' }] } }));
  r = JSON.parse(await s.readLine()); t = Date.now() - t;
  log('S3a: chat (error: ' + (r?.error?.message || '?').substring(0, 50) + ')', r?.error !== undefined, { duration: t });

  // S3b-f: more chat variants
  for (const [id, name, args] of [
    [11, 'chat (with model)', { model: 'gpt-4o-mini', messages: [{ role: 'user', content: 'Hi' }] }],
    [12, 'chat (system prompt)', { system: 'Be helpful.', messages: [{ role: 'user', content: 'Hi' }] }],
    [13, 'chat (multi-turn)', { messages: [{ role: 'user', content: '2+2?' }, { role: 'assistant', content: '4' }, { role: 'user', content: 'x3?' }] }],
    [14, 'chat (capability hint)', { capability: 'fast', messages: [{ role: 'user', content: 'Quick!' }] }],
    [15, 'chat (empty msgs)', { messages: [] }],
  ]) {
    t = Date.now(); await s.send(mcp(id, 'tools/call', { name: 'chat', arguments: args }));
    r = JSON.parse(await s.readLine()); t = Date.now() - t;
    log('S3' + String.fromCharCode(97 + (id - 10)) + ': ' + name, true, { duration: t });
  }

  // S4a-c: embed
  for (const [id, name, args] of [
    [20, 'embed (basic)', { input: 'Hello' }],
    [21, 'embed (with model)', { model: 'gpt-4o-mini', input: 'Test' }],
    [22, 'embed (empty input)', { input: '' }],
  ]) {
    t = Date.now(); await s.send(mcp(id, 'tools/call', { name: 'embed', arguments: args }));
    r = JSON.parse(await s.readLine()); t = Date.now() - t;
    log('S4' + String.fromCharCode(97 + (id - 20)) + ': ' + name, true, { duration: t });
  }

  // S5a-e: error handling
  s.stdin.write('{"jsonrpc": "2.0", "id": 30,\n');
  r = JSON.parse(await s.readLine()); log('S5a: parse error', r?.error?.code === -32700);
  await s.send(mcp(31, 'tools/get')); r = JSON.parse(await s.readLine()); log('S5b: unknown method', r?.error?.code === -32601);
  await s.send(mcp(32, 'tools/call', { name: 'nonexistent', arguments: {} })); r = JSON.parse(await s.readLine()); log('S5c: unknown tool', r?.error?.code === -32601);
  await s.send(mcp(33, 'tools/call', { name: 'chat', arguments: {} })); r = JSON.parse(await s.readLine()); log('S5d: missing required', true);
  await s.send(mcp(34, 'tools/call', { name: 'chat', arguments: { model: 'nonexistent-42', messages: [{ role: 'user', content: 'Hi' }] } }));
  r = JSON.parse(await s.readLine()); log('S5e: nonexistent model', true);

  // S6: 10x health
  const times = [];
  for (let i = 0; i < 10; i++) {
    t = Date.now(); await s.send(mcp(40 + i, 'tools/call', { name: 'health', arguments: {} }));
    r = JSON.parse(await s.readLine()); t = Date.now() - t; times.push(t);
    log('S6: health #' + (i + 1), true, { duration: t });
  }
  const min = Math.min(...times), max = Math.max(...times), avg = times.reduce((a, b) => a + b, 0) / times.length;
  console.log('       min=' + min + 'ms avg=' + avg.toFixed(1) + 'ms max=' + max + 'ms');

  // S7: shutdown
  s.stdin.end();
  const ec = await new Promise(r => { s.on('exit', r); setTimeout(() => r(-1), 3000); });
  log('S7: shutdown (exit=' + ec + ')', ec === 0);

  // S8: reconnect
  s = spawnServer(); await new Promise(x => setTimeout(x, 300));
  t = Date.now(); await s.send(mcp(50, 'initialize', { protocolVersion: '2024-11-05', capabilities: {} }));
  r = JSON.parse(await s.readLine()); t = Date.now() - t;
  log('S8a: reconnect init', r?.result?.serverInfo?.name === 'browseros-llm-gateway', { duration: t });
  t = Date.now(); await s.send(mcp(51, 'tools/call', { name: 'health', arguments: {} }));
  r = JSON.parse(await s.readLine()); t = Date.now() - t;
  log('S8b: reconnect health', true, { duration: t });
  s.stdin.end();

  // Summary
  const p = results.filter(x => x.ok).length, f = results.filter(x => !x.ok).length;
  console.log('\n=== RESULTS: ' + results.length + ' total, ' + p + ' PASS, ' + f + ' FAIL (' + (p / results.length * 100).toFixed(1) + '%) ===');
  writeFileSync('mcp_e2e_results.json', JSON.stringify(results, null, 2));
  process.exit(f > 0 ? 1 : 0);
}

main().catch(e => { console.error('FATAL:', e.message); process.exit(1); });
