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
const nextBin = require.resolve('next/dist/bin/next');

const host = '127.0.0.1';
const publicPort = 3118;
const preferredNextPort = 3119;
let nextPort = preferredNextPort;
const warmupPaths = [
  '/',
  '/_next/static/chunks/main-app.js',
  '/_next/static/chunks/app-pages-internals.js',
  '/_next/static/chunks/app/layout.js',
  '/_next/static/chunks/app/page.js',
  '/_next/static/chunks/webpack.js',
];

let proxy;
let next;
let stopping = false;

function stopExistingProxy() {
  return new Promise((resolve) => {
    const replacementRequest = http.request(
      {
        host,
        port: publicPort,
        path: '/__memento_dev_shutdown__',
        method: 'POST',
        timeout: 500,
      },
      (response) => {
        response.resume();
        response.on('end', () => {
          if (response.statusCode === 204) {
            console.log(`Replacing the previous Memento dev server on port ${publicPort}.`);
            setTimeout(resolve, 200);
          } else {
            resolve();
          }
        });
      },
    );
    replacementRequest.on('error', resolve);
    replacementRequest.on('timeout', () => replacementRequest.destroy());
    replacementRequest.end();
  });
}

function isPortAvailable(port) {
  return new Promise((resolve) => {
    const probe = net.createServer();
    probe.unref();
    probe.once('error', () => resolve(false));
    probe.listen(port, host, () => {
      probe.close(() => resolve(true));
    });
  });
}

async function findAvailablePort(startPort, attempts = 20) {
  for (let port = startPort; port < startPort + attempts; port += 1) {
    if (await isPortAvailable(port)) return port;
  }
  throw new Error(`No free Next dev port found in ${startPort}-${startPort + attempts - 1}`);
}

async function waitForPortAvailable(port, attempts = 20) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    if (await isPortAvailable(port)) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    `Port ${port} is occupied by another process. Stop the previous Memento dev server and try again.`,
  );
}

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
  if (clientRequest.method === 'POST' && clientRequest.url === '/__memento_dev_shutdown__') {
    clientResponse.writeHead(204);
    clientResponse.end();
    setImmediate(() => {
      stop();
      process.exit(0);
    });
    return;
  }

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
    await stopExistingProxy();
    await waitForPortAvailable(publicPort);
    nextPort = await findAvailablePort(preferredNextPort);
    if (nextPort !== preferredNextPort) {
      console.log(`Port ${preferredNextPort} is busy; using ${nextPort} for Next dev.`);
    }

    next = spawn(
      process.execPath,
      [nextBin, 'dev', '-H', host, '-p', String(nextPort)],
      { stdio: 'inherit' },
    );
    next.on('exit', (code) => {
      if (!stopping) process.exit(code ?? 1);
    });

    console.log('⏳ Warming Next route and client chunks for Tauri…');
    await warmup();

    proxy = http.createServer(forwardRequest);
    proxy.on('upgrade', forwardUpgrade);
    proxy.listen(publicPort, host, () => {
      console.log(`✓ Tauri dev server ready at http://${host}:${publicPort}`);
    });
  } catch (error) {
    console.error('Failed to prepare Next for Tauri:', error);
    next?.kill('SIGTERM');
    process.exitCode = 1;
  }
}

function stop() {
  if (stopping) return;
  stopping = true;
  proxy?.close();
  next?.kill('SIGTERM');
}

process.on('SIGINT', stop);
process.on('SIGTERM', stop);

void start();
