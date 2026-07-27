const connection = document.querySelector('#connection');
const statusDot = document.querySelector('#status-dot');
const tabTitle = document.querySelector('#tab-title');
const result = document.querySelector('#result');
const selectButton = document.querySelector('#select-tab');
const startButton = document.querySelector('#start');
const stopButton = document.querySelector('#stop');
const deleteButton = document.querySelector('#delete');

let latestStatus;
let refreshing = false;

function render(status = {}) {
  latestStatus = status;
  connection.textContent = status.bridgeConnected
    ? status.recording
      ? status.origin === 'meeting'
        ? 'Meeting recording active'
        : 'Standalone recording active'
      : 'Meetily connected'
    : 'Open Meetily to record';
  statusDot.classList.toggle('connected', Boolean(status.bridgeConnected));
  tabTitle.textContent = status.title || 'None';
  selectButton.disabled = Boolean(status.recording);
  startButton.disabled = !status.bridgeConnected || !status.armed || Boolean(status.recording);
  stopButton.disabled = !status.recording;
  const savedRecordingAvailable = status.lastResult?.startsWith('Saved ');
  deleteButton.disabled = !status.bridgeConnected
    || (!status.recording && !savedRecordingAvailable);
  result.textContent = status.lastResult
    || (status.armed
      ? 'Ready. Start here or begin a meeting in Meetily.'
      : 'Select a tab to begin.');
}

async function send(type) {
  const response = await chrome.runtime.sendMessage({ target: 'popup', type });
  if (!response?.ok) throw new Error(response?.error || 'Capture command failed');
  render(response.status);
}

async function run(type) {
  try {
    result.textContent = 'Working…';
    await send(type);
  } catch (error) {
    result.textContent = error.message;
  }
}

async function refresh() {
  if (refreshing) return;
  refreshing = true;
  try {
    await send('status');
  } catch (error) {
    render(latestStatus);
    result.textContent = error.message;
  } finally {
    refreshing = false;
  }
}

selectButton.addEventListener('click', () => run('arm_current'));
startButton.addEventListener('click', () => run('manual_start'));
stopButton.addEventListener('click', () => run('stop_user'));
deleteButton.addEventListener('click', () => run('delete_user'));

refresh();
window.setInterval(refresh, 750);
