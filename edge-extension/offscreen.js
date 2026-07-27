const BRIDGE_URL = 'ws://127.0.0.1:8179';
const BRIDGE_TOKEN = 'meetily-local-capture-v1-7d4f2c9a6e31b805';

let socket;
let reconnectTimer;
let capturedStream;
let mediaRecorder;
let armedTitle;
let audioContext;
let chunkWrites = Promise.resolve();

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
  });
  socket.addEventListener('message', async (event) => {
    const command = JSON.parse(event.data);
    if (command.type === 'start') await startRecording();
    if (command.type === 'stop') stopRecording();
  });
  socket.addEventListener('close', () => {
    reconnectTimer = setTimeout(connect, 1000);
  });
  socket.addEventListener('error', () => socket.close());
}

function send(message) {
  if (socket?.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify({ ...message, token: BRIDGE_TOKEN }));
  }
}

async function arm(streamId, title) {
  if (mediaRecorder?.state === 'recording') {
    throw new Error('Cannot change tabs while recording');
  }
  capturedStream?.getTracks().forEach((track) => track.stop());
  if (audioContext) await audioContext.close();

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
}

async function startRecording() {
  if (!capturedStream?.active) {
    send({ type: 'error', message: 'The armed tab is no longer available' });
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
    send({ type: 'complete' });
    chrome.runtime.sendMessage({ target: 'service-worker', type: 'armed' });
  }, { once: true });
  mediaRecorder.start(1000);
  chrome.runtime.sendMessage({ target: 'service-worker', type: 'recording' });
}

function stopRecording() {
  if (mediaRecorder?.state === 'recording') {
    mediaRecorder.stop();
  } else {
    send({ type: 'complete' });
  }
}

function disarm() {
  if (mediaRecorder?.state === 'recording') mediaRecorder.stop();
  capturedStream = undefined;
  armedTitle = undefined;
  send({ type: 'disarmed' });
  chrome.runtime.sendMessage({ target: 'service-worker', type: 'disarmed' });
}

chrome.runtime.onMessage.addListener((message) => {
  if (message.target !== 'offscreen' || message.type !== 'arm') return;
  arm(message.streamId, message.title).catch((error) => {
    send({ type: 'error', message: error.message });
  });
});

connect();
