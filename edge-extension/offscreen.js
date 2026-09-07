const BRIDGE_URL = 'ws://127.0.0.1:8179';
const BRIDGE_TOKEN = 'meetily-local-capture-v1-7d4f2c9a6e31b805';

let socket;
let reconnectTimer;
let capturedStream;
let mediaRecorder;
let armedTitle;
let audioContext;
let chunkWrites = Promise.resolve();
let releasingCapture = false;
let captureOrigin;
let discardOnStop = false;
let lastResult;

function connect() {
  clearTimeout(reconnectTimer);
  socket = new WebSocket(BRIDGE_URL);
  socket.binaryType = 'arraybuffer';
  socket.addEventListener('open', () => {
    if (capturedStream?.active) {
      send({ type: 'armed', title: armedTitle });
    } else {
      send({ type: 'hello' });
    }
    notifyState();
  });
  socket.addEventListener('message', async (event) => {
    const command = JSON.parse(event.data);
    if (command.type === 'start') {
      await startRecording(command.origin || 'meeting');
    } else if (command.type === 'stop') {
      stopRecording(false);
    } else if (command.type === 'saved') {
      lastResult = `Saved ${command.filename}`;
      notifyState();
    } else if (command.type === 'discarded') {
      lastResult = 'Recording deleted';
      notifyState();
    } else if (command.type === 'error') {
      lastResult = command.message || 'Capture failed';
      notifyState();
    }
  });
  socket.addEventListener('close', () => {
    notifyState();
    reconnectTimer = setTimeout(connect, 1000);
  });
  socket.addEventListener('error', () => socket.close());
}

function send(message) {
  if (socket?.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify({ ...message, token: BRIDGE_TOKEN }));
    return true;
  }
  return false;
}

function currentStatus() {
  return {
    bridgeConnected: socket?.readyState === WebSocket.OPEN,
    armed: Boolean(capturedStream?.active),
    recording: mediaRecorder?.state === 'recording',
    origin: captureOrigin || null,
    title: armedTitle || null,
    lastResult: lastResult || null,
  };
}

function notifyState() {
  chrome.runtime.sendMessage({
    target: 'service-worker',
    type: 'state',
    status: currentStatus(),
  }).catch(() => {});
}

async function arm(streamId, title) {
  if (mediaRecorder?.state === 'recording') {
    throw new Error('Stop the current recording before changing tabs');
  }

  capturedStream = await navigator.mediaDevices.getUserMedia({
    audio: {
      mandatory: {
        chromeMediaSource: 'tab',
        chromeMediaSourceId: streamId,
      },
    },
    video: {
      mandatory: {
        chromeMediaSource: 'tab',
        chromeMediaSourceId: streamId,
      },
    },
  });
  armedTitle = title;
  lastResult = 'Tab selected';

  const audioTracks = capturedStream.getAudioTracks();
  if (audioTracks.length > 0) {
    audioContext = new AudioContext();
    const source = audioContext.createMediaStreamSource(
      new MediaStream(audioTracks),
    );
    source.connect(audioContext.destination);
  }

  for (const track of capturedStream.getTracks()) {
    track.addEventListener('ended', disarm, { once: true });
  }
  send({ type: 'armed', title: armedTitle });
  chrome.runtime.sendMessage({ target: 'service-worker', type: 'armed' }).catch(() => {});
  notifyState();
}

async function releaseCapture() {
  if (mediaRecorder?.state === 'recording') {
    throw new Error('Stop the current recording before changing tabs');
  }

  releasingCapture = true;
  capturedStream?.getTracks().forEach((track) => track.stop());
  capturedStream = undefined;
  armedTitle = undefined;
  if (audioContext) {
    await audioContext.close();
    audioContext = undefined;
  }
  releasingCapture = false;
  send({ type: 'disarmed' });
  notifyState();
}

async function startRecording(origin) {
  if (!capturedStream?.active) {
    lastResult = 'The selected tab is no longer available';
    send({ type: 'error', message: lastResult });
    notifyState();
    return;
  }
  if (mediaRecorder?.state === 'recording') return;

  const mimeType = [
    'video/webm;codecs=vp9,opus',
    'video/webm;codecs=vp8,opus',
    'video/webm',
  ].find((candidate) => MediaRecorder.isTypeSupported(candidate));
  mediaRecorder = new MediaRecorder(
    capturedStream,
    mimeType ? { mimeType, videoBitsPerSecond: 5_000_000 } : undefined,
  );
  captureOrigin = origin;
  discardOnStop = false;
  lastResult = null;
  chunkWrites = Promise.resolve();
  mediaRecorder.addEventListener('dataavailable', (event) => {
    if (event.data.size > 0 && socket?.readyState === WebSocket.OPEN) {
      chunkWrites = chunkWrites.then(async () => {
        socket.send(await event.data.arrayBuffer());
      });
    }
  });
  mediaRecorder.addEventListener('stop', async () => {
    await chunkWrites;
    send({ type: discardOnStop ? 'discard' : 'complete' });
    lastResult = discardOnStop
      ? 'Deleting recording…'
      : captureOrigin === 'standalone'
        ? 'Saving recording…'
        : 'Saved with the meeting';
    captureOrigin = undefined;
    discardOnStop = false;
    chrome.runtime.sendMessage({ target: 'service-worker', type: 'armed' }).catch(() => {});
    notifyState();
  }, { once: true });
  mediaRecorder.start(1000);
  chrome.runtime.sendMessage({ target: 'service-worker', type: 'recording' }).catch(() => {});
  notifyState();
}

function stopRecording(discard) {
  if (mediaRecorder?.state === 'recording') {
    discardOnStop = discard;
    mediaRecorder.stop();
  } else if (discard) {
    send({ type: 'discard' });
    lastResult = 'Deleting recording…';
    notifyState();
  } else {
    send({ type: 'complete' });
  }
}

function disarm() {
  if (releasingCapture) return;
  if (mediaRecorder?.state === 'recording') mediaRecorder.stop();
  capturedStream = undefined;
  armedTitle = undefined;
  send({ type: 'disarmed' });
  chrome.runtime.sendMessage({ target: 'service-worker', type: 'disarmed' }).catch(() => {});
  notifyState();
}

async function handleMessage(message) {
  if (message.type === 'release') {
    await releaseCapture();
  } else if (message.type === 'arm') {
    await arm(message.streamId, message.title);
  } else if (message.type === 'status') {
    return currentStatus();
  } else if (message.type === 'manual_start') {
    if (!capturedStream?.active) throw new Error('Select a tab first');
    if (!send({ type: 'manual_start' })) throw new Error('Open Meetily first');
    lastResult = 'Starting recording…';
    notifyState();
  } else if (message.type === 'stop_user') {
    if (mediaRecorder?.state !== 'recording') throw new Error('No recording is active');
    stopRecording(false);
  } else if (message.type === 'delete_user') {
    stopRecording(true);
  }
  return currentStatus();
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.target !== 'offscreen') return;
  handleMessage(message)
    .then((status) => sendResponse({ ok: true, status }))
    .catch((error) => {
      lastResult = error.message;
      notifyState();
      sendResponse({ ok: false, error: error.message, status: currentStatus() });
    });
  return true;
});

connect();
