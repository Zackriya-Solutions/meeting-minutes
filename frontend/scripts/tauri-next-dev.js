#!/usr/bin/env node

/**
 * Starts Next on a private port, compiles the initial route and its client
 * chunks, then exposes it through the port consumed by Tauri.
 *
 * Tauri considers a dev server ready as soon as `/` answers. Next, however,
 * compiles route chunks lazily. Launching the WebView in that gap can make it
 * time out while loading `app/layout.js`, leaving a static page with no React
 * handlers. The small proxy keeps port 3118 closed until those chunks exist.
 */
const http = require('http');
const net = require('net');
const { spawn } = require('child_process');

const host = '127.0.0.1';
const publicPort = 3118;
const nextPort = 3119;
const warmupPaths = [
  '/',
  '/_next/static/chunks/main-app.js',
  '/_next/static/chunks/app-pages-internals.js',
  '/_next/static/chunks/app/layout.js',
  '/_next/static/chunks/app/page.js',
  '/_next/static/chunks/webpack.js',
];

const next = spawn(
  process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm',
  ['exec', 'next', 'dev', '-H', host, '-p', String(nextPort)],
  { stdio: 'inherit' },
);

let proxy;
let stopping = false;

function request(pathname) {
  return new Promise((resolve, reject) => {
    const request = http.get({ host, port: nextPort, path: pathname }, (response) => {
      response.resume();
      response.on('end', () => {
        if (response.statusCode >= 200 && response.statusCode < 400) resolve();
        else reject(new Error(`${pathname} returned ${response.statusCode}`));
      });
    });
    request.on('error', reject);
    request.setTimeout(1_000, () => request.destroy(new Error('request timed out')));
  });
}

async function warmup() {
  for (;;) {
    try {
      await request('/');
      break;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
  }

  for (const pathname of warmupPaths.slice(1)) {
    await request(pathname);
  }
}

function forwardRequest(clientRequest, clientResponse) {
  const upstreamRequest = http.request(
    {
      host,
      port: nextPort,
      path: clientRequest.url,
      method: clientRequest.method,
      headers: clientRequest.headers,
    },
    (upstreamResponse) => {
      clientResponse.writeHead(upstreamResponse.statusCode ?? 502, upstreamResponse.headers);
      upstreamResponse.pipe(clientResponse);
    },
  );

  upstreamRequest.on('error', () => {
    if (!clientResponse.headersSent) clientResponse.writeHead(502);
    clientResponse.end('Next dev server is unavailable');
  });
  clientRequest.pipe(upstreamRequest);
}

function forwardUpgrade(request, socket, head) {
  const upstream = net.connect(nextPort, host, () => {
    const headers = Object.entries(request.headers)
      .map(([name, value]) => `${name}: ${Array.isArray(value) ? value.join(', ') : value}`)
      .join('\r\n');
    upstream.write(`${request.method} ${request.url} HTTP/${request.httpVersion}\r\n${headers}\r\n\r\n`);
    if (head.length) upstream.write(head);
    socket.pipe(upstream).pipe(socket);
  });

  const close = () => upstream.destroy();
  socket.on('error', close);
  upstream.on('error', () => socket.destroy());
}

async function start() {
  try {
    console.log('⏳ Warming Next route and client chunks for Tauri…');
    await warmup();

    proxy = http.createServer(forwardRequest);
    proxy.on('upgrade', forwardUpgrade);
    proxy.listen(publicPort, host, () => {
      console.log(`✓ Tauri dev server ready at http://${host}:${publicPort}`);
    });
  } catch (error) {
    console.error('Failed to prepare Next for Tauri:', error);
    next.kill('SIGTERM');
    process.exitCode = 1;
  }
}

function stop() {
  if (stopping) return;
  stopping = true;
  proxy?.close();
  next.kill('SIGTERM');
}

process.on('SIGINT', stop);
process.on('SIGTERM', stop);
next.on('exit', (code) => process.exit(code ?? 0));

void start();
