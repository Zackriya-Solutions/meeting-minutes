// Floating recording indicator pill.
// Driven by "mic-level" events emitted by the Rust backend while recording.
//
// Uses the low-level __TAURI_INTERNALS__ IPC directly (always injected by Tauri)
// instead of the withGlobalTauri wrapper, so the main window keeps its original
// JS environment untouched.

(function () {
  const internals = window.__TAURI_INTERNALS__;
  if (!internals || typeof internals.invoke !== 'function') {
    console.error('[FloatingIndicator] __TAURI_INTERNALS__ not available');
    return;
  }

  const WINDOW_LABEL = 'floating-indicator';

  function invoke(cmd, args) {
    return internals.invoke(cmd, args || {});
  }

  // Minimal event listener built on the core event plugin.
  function listen(eventName, handler) {
    const callbackId = internals.transformCallback((e) => handler(e));
    return invoke('plugin:event|listen', {
      event: eventName,
      target: { kind: 'Any' },
      handler: callbackId,
    });
  }

  function startDragging() {
    return invoke('plugin:window|start_dragging', { label: WINDOW_LABEL });
  }

  const BAR_COUNT = 10;
  const MIN_BAR_PX = 2;
  const MAX_BAR_PX = 15;

  const pillEl = document.getElementById('pill');
  const waveEl = document.getElementById('wave');
  const timeEl = document.getElementById('time');
  const stopBtn = document.getElementById('stop');

  // Build waveform bars
  const bars = [];
  for (let i = 0; i < BAR_COUNT; i++) {
    const bar = document.createElement('div');
    bar.className = 'bar';
    waveEl.appendChild(bar);
    bars.push(bar);
  }

  // Scrolling level history: newest sample enters on the right
  const levels = new Array(BAR_COUNT).fill(0);

  function scaleLevel(rms) {
    // Raw speech RMS is typically 0.005–0.15; sqrt gives a livelier wave
    return Math.min(1, Math.sqrt(rms * 12));
  }

  function renderWave() {
    for (let i = 0; i < BAR_COUNT; i++) {
      const h = MIN_BAR_PX + levels[i] * (MAX_BAR_PX - MIN_BAR_PX);
      bars[i].style.height = h.toFixed(1) + 'px';
    }
  }

  function formatDuration(seconds) {
    if (seconds == null || !isFinite(seconds)) return '0:00';
    const total = Math.max(0, Math.floor(seconds));
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    const ss = String(s).padStart(2, '0');
    return h > 0 ? h + ':' + String(m).padStart(2, '0') + ':' + ss : m + ':' + ss;
  }

  let stopping = false;

  listen('mic-level', (event) => {
    if (stopping) return;
    const { rms, is_paused: isPaused, duration } = event.payload;

    levels.shift();
    levels.push(isPaused ? 0 : scaleLevel(rms));
    renderWave();

    timeEl.textContent = isPaused ? 'Paused' : formatDuration(duration);
    document.body.classList.toggle('paused', !!isPaused);
  });

  // Reset to a clean state when a new recording starts
  listen('recording-started', () => {
    stopping = false;
    stopBtn.disabled = false;
    document.body.classList.remove('stopping', 'paused');
    levels.fill(0);
    renderWave();
    timeEl.textContent = '0:00';
  });

  // Drag handling.
  //
  // While dragging, macOS activates the app and focuses the main window, which
  // would make the backend hide the pill mid-drag (it would then drag invisibly
  // in the background). We tell the backend a drag is in progress so it keeps
  // the pill visible. Exact end timing isn't critical: once the drag ends the
  // main window is no longer focused, so the pill stays visible anyway.
  let dragSafetyTimer = null;

  function setDragging(active) {
    invoke('set_indicator_dragging', { dragging: active }).catch(() => {});
  }

  function endDrag() {
    if (dragSafetyTimer) { clearTimeout(dragSafetyTimer); dragSafetyTimer = null; }
    setDragging(false);
  }

  function armSafety() {
    if (dragSafetyTimer) clearTimeout(dragSafetyTimer);
    // Fallback in case a mouseup is swallowed by the OS drag loop.
    dragSafetyTimer = setTimeout(() => { setDragging(false); dragSafetyTimer = null; }, 1500);
  }

  pillEl.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    if (stopBtn.contains(e.target)) return;
    // Prevent the default focus/activation so macOS doesn't raise the main window.
    e.preventDefault();
    setDragging(true);
    armSafety();
    startDragging()
      .catch((err) => {
        console.error('[FloatingIndicator] startDragging failed:', err);
        endDrag();
      });
  });

  // Clear the drag flag once the mouse is released or the window stops moving.
  document.addEventListener('mouseup', endDrag);
  listen('tauri://move', () => { if (dragSafetyTimer) armSafety(); });

  stopBtn.addEventListener('click', async () => {
    if (stopping) return;
    stopping = true;
    stopBtn.disabled = true;
    document.body.classList.add('stopping');
    timeEl.textContent = 'Stopping…';
    try {
      await invoke('stop_recording_from_indicator');
    } catch (e) {
      console.error('[FloatingIndicator] Failed to stop recording:', e);
      stopping = false;
      stopBtn.disabled = false;
      document.body.classList.remove('stopping');
    }
  });

  renderWave();
})();
