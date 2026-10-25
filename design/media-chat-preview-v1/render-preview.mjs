import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const directory = path.dirname(fileURLToPath(import.meta.url));
const edgePath = process.env.LAP_PREVIEW_EDGE || 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe';
const screens = [
  { screen: 'chat', width: 1600, height: 1000, output: '01-chat.png' },
  { screen: 'attach', width: 1200, height: 900, output: '02-attach.png' },
  { screen: 'review', width: 1600, height: 1000, output: '03-review.png' },
  { screen: 'entry', width: 1600, height: 1000, output: '04-entry.png' },
  { screen: 'viewer', width: 1600, height: 1000, output: '05-viewer.png' },
  { screen: 'models', width: 1600, height: 1000, output: '06-models.png' },
];
const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
let browser;
let socket;
let requestId = 0;
const pending = new Map();
const exceptions = [];

async function until(check, description, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try { if (await check()) return; } catch { /* A navigation can replace its execution context. */ }
    await pause(100);
  }
  throw new Error(`Timed out: ${description}`);
}

function send(method, params = {}) {
  const id = ++requestId;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}

const mimeTypes = { '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.svg': 'image/svg+xml', '.png': 'image/png', '.woff2': 'font/woff2' };
const server = http.createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    const target = path.resolve(directory, `.${pathname === '/' ? '/index.html' : pathname}`);
    if (!target.startsWith(directory + path.sep)) { response.writeHead(403).end(); return; }
    const contents = await fs.readFile(target);
    response.writeHead(200, { 'Content-Type': mimeTypes[path.extname(target)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
    response.end(contents);
  } catch { response.writeHead(404).end('Not found'); }
});

try {
  await fs.access(path.join(directory, 'index.html'));
  await fs.access(edgePath);
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  const profile = await fs.mkdtemp(path.join(os.tmpdir(), 'lap-media-chat-preview-'));
  browser = spawn(edgePath, ['--headless=new', '--remote-debugging-port=0', '--remote-allow-origins=*', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--disable-gpu', 'about:blank'], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
  let startupError = '';
  browser.stderr.on('data', chunk => { startupError = (startupError + chunk).slice(-4000); });
  const portFile = path.join(profile, 'DevToolsActivePort');
  await until(async () => { await fs.access(portFile); return true; }, `Edge startup (${startupError})`);
  const port = (await fs.readFile(portFile, 'utf8')).split(/\r?\n/)[0];
  const tabs = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  socket = new WebSocket(tabs.find(tab => tab.type === 'page').webSocketDebuggerUrl);
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const operation = pending.get(message.id);
      if (!operation) return;
      clearTimeout(operation.timer); pending.delete(message.id);
      if (message.error) operation.reject(new Error(JSON.stringify(message.error)));
      else operation.resolve(message.result);
    } else if (message.method === 'Runtime.exceptionThrown') exceptions.push(message.params);
  });
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
  await send('Page.enable');
  await send('Runtime.enable');
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
  const results = [];
  for (const scene of screens) {
    await send('Emulation.setDeviceMetricsOverride', { width: scene.width, height: scene.height, deviceScaleFactor: 1, mobile: false });
    await send('Page.navigate', { url: `${origin}/?screen=${scene.screen}` });
    await until(() => evaluate(`location.search === '?screen=${scene.screen}' && document.readyState === 'complete'`), scene.screen);
    await evaluate('Promise.race([document.fonts.ready, new Promise(resolve => setTimeout(resolve, 3000))]).then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))');
    const layout = await evaluate(`({ title: document.title, viewport: [innerWidth, innerHeight], document: [document.documentElement.scrollWidth, document.documentElement.scrollHeight], text: document.body.innerText.slice(0, 500), chineseCharacters: (document.body.innerText.match(/\\p{Script=Han}/gu) || []).length, fonts: [...document.fonts].map(font => ({ family: font.family, status: font.status })) })`);
    if (layout.text.trim().length < 20) throw new Error(`${scene.screen}: page text is missing`);
    const screenshot = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
    await fs.writeFile(path.join(directory, scene.output), Buffer.from(screenshot.data, 'base64'));
    results.push({ ...scene, ...layout });
    console.log(`${scene.output}: ${JSON.stringify(layout)}`);
  }
  if (process.argv.includes('--probe')) {
    for (const width of [1200, 1051, 1050]) {
      await send('Emulation.setDeviceMetricsOverride', { width, height: 900, deviceScaleFactor: 1, mobile: false });
      await send('Page.navigate', { url: `${origin}/?screen=chat` });
      await until(() => evaluate("location.search === '?screen=chat' && document.readyState === 'complete'"), `chat at ${width}px`);
      const layout = await evaluate("(() => { const r = document.querySelector('.chat-window').getBoundingClientRect(); return { viewport: innerWidth, x: r.x, width: r.width, right: r.right, overflowRight: Math.max(0, r.right - innerWidth) }; })()");
      results.push({ screen: 'chat-probe', ...layout });
      console.log(`chat-probe: ${JSON.stringify(layout)}`);
    }
  }
  await fs.writeFile(path.join(directory, 'render-report.json'), JSON.stringify({ renderedAt: new Date().toISOString(), results, exceptions }, null, 2));
  if (exceptions.length) throw new Error(`${exceptions.length} browser exceptions; see render-report.json`);
  console.log(`Rendered ${screens.length} previews in ${directory}`);
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  if (socket?.readyState === WebSocket.OPEN) { try { await send('Browser.close'); } catch { /* Continue local cleanup. */ } socket.close(); }
  if (browser && !browser.killed) browser.kill();
  for (const operation of pending.values()) clearTimeout(operation.timer);
  server.close();
}
