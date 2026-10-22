// A stand-in agent app for Atlas's MCP view, for live checks: it starts
// `atlas mcp`, serves a page that shows the view in a sandboxed frame and
// speaks the MCP Apps bridge with it, and passes the view's tool calls to
// Atlas. Open the printed address in a browser in the helper and look.
//
//   node scripts/check-mcp-view.mjs <atlas> <config> [--theme dark] [--fullscreen]
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { createInterface } from 'node:readline';

const [atlas, config, ...flags] = process.argv.slice(2);
if (!atlas || !config) {
  console.error(
    'usage: node scripts/check-mcp-view.mjs <atlas> <config> [--theme dark] [--fullscreen]',
  );
  process.exit(2);
}
const theme = flags.includes('--theme') ? flags[flags.indexOf('--theme') + 1] : 'light';
const fullscreen = flags.includes('--fullscreen');

const server = spawn(atlas, ['--config', config, 'mcp'], { stdio: ['pipe', 'pipe', 'inherit'] });
const waiting = new Map();
let nextId = 1;
createInterface({ input: server.stdout }).on('line', (line) => {
  const message = JSON.parse(line);
  waiting.get(message.id)?.(message);
  waiting.delete(message.id);
});
const ask = (method, params) =>
  new Promise((resolve) => {
    const id = nextId++;
    waiting.set(id, resolve);
    server.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
  });

const started = await ask('initialize', {
  protocolVersion: '2025-11-25',
  capabilities: {
    extensions: { 'io.modelcontextprotocol/ui': { mimeTypes: ['text/html;profile=mcp-app'] } },
  },
  clientInfo: { name: 'atlas-check', version: '1' },
});
console.error('initialized:', JSON.stringify(started.result.serverInfo));
const view = (await ask('resources/read', { uri: 'ui://atlas/screen.html' })).result.contents[0]
  .text;
const shown = (await ask('tools/call', { name: 'show_screen', arguments: {} })).result;
console.error('show_screen says:', shown.content[0].text);

const host = `<!doctype html>
<html><head><meta charset="utf-8"><title>Atlas view check</title>
<style>
  body { margin: 0; font: 13px -apple-system, sans-serif; background: ${theme === 'dark' ? '#111' : '#f5f5f4'};
    color: ${theme === 'dark' ? '#eee' : '#222'}; }
  .chat { max-width: 720px; margin: 24px auto; display: flex; flex-direction: column; gap: 12px; }
  iframe { border: 0; width: 100%; height: 200px; background: transparent; }
  body.full .chat { max-width: none; margin: 0; }
  body.full .chat > div { display: none; }
  body.full iframe { height: 100vh; }
  #log { position: fixed; right: 8px; bottom: 8px; font: 11px ui-monospace, monospace; opacity: 0.7; }
</style></head>
<body class="${fullscreen ? 'full' : ''}">
<div class="chat">
  <div>Check the new Save dialog in the helper.</div>
  <iframe id="view" sandbox="allow-scripts" srcdoc="${view.replace(/&/g, '&amp;').replace(/"/g, '&quot;')}"></iframe>
</div>
<div id="log"></div>
<script>
const frame = document.getElementById("view");
const log = (text) => (document.getElementById("log").textContent = text);
let mode = ${JSON.stringify(fullscreen ? 'fullscreen' : 'inline')};
const reply = (id, result) => frame.contentWindow.postMessage({ jsonrpc: "2.0", id, result }, "*");
const tell = (method, params) => frame.contentWindow.postMessage({ jsonrpc: "2.0", method, params }, "*");
window.addEventListener("message", async (event) => {
  if (event.source !== frame.contentWindow) return;
  const m = event.data;
  if (m.method === "ui/initialize") {
    reply(m.id, { protocolVersion: "2026-01-26", hostInfo: { name: "atlas-check", version: "1" },
      hostCapabilities: { serverTools: {}, openLinks: {} },
      hostContext: { theme: ${JSON.stringify(theme)}, displayMode: mode,
        availableDisplayModes: ["inline", "fullscreen"], platform: "desktop" } });
  } else if (m.method === "ui/notifications/initialized") {
    tell("ui/notifications/tool-result", ${JSON.stringify(shown)});
    log("initialized");
  } else if (m.method === "tools/call") {
    const started = Date.now();
    const result = await (await fetch("/call", { method: "POST", body: JSON.stringify(m.params) })).json();
    log(m.params.name + " " + (Date.now() - started) + " ms");
    reply(m.id, result);
  } else if (m.method === "ui/request-display-mode") {
    mode = m.params.mode;
    document.body.classList.toggle("full", mode === "fullscreen");
    reply(m.id, { mode });
  } else if (m.method === "ui/notifications/size-changed") {
    if (mode !== "fullscreen") frame.style.height = m.params.height + "px";
  }
});
</script></body></html>`;

const http = createServer(async (request, response) => {
  if (request.method === 'POST' && request.url === '/call') {
    let body = '';
    for await (const chunk of request) body += chunk;
    const answer = await ask('tools/call', JSON.parse(body));
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(
      JSON.stringify(
        answer.result ?? { isError: true, content: [{ type: 'text', text: 'no answer' }] },
      ),
    );
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  response.end(host);
});
http.listen(0, '127.0.0.1', () => console.log(`http://127.0.0.1:${http.address().port}/`));
process.on('SIGTERM', () => {
  server.kill();
  process.exit(0);
});
