const OFFSCREEN_DOCUMENT = 'offscreen.html';

async function ensureOffscreenDocument() {
  const offscreenUrl = chrome.runtime.getURL(OFFSCREEN_DOCUMENT);
  const contexts = await chrome.runtime.getContexts({
    contextTypes: ['OFFSCREEN_DOCUMENT'],
    documentUrls: [offscreenUrl],
  });
  if (contexts.length === 0) {
    await chrome.offscreen.createDocument({
      url: OFFSCREEN_DOCUMENT,
      reasons: ['USER_MEDIA'],
      justification: 'Record the exact tab selected by the user for Meetily',
    });
  }
}

async function armCurrentTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id) throw new Error('No active tab is available');

  await ensureOffscreenDocument();
  const releaseResult = await chrome.runtime.sendMessage({
    target: 'offscreen',
    type: 'release',
  });
  if (!releaseResult?.ok) {
    throw new Error(releaseResult?.error || 'Unable to release the previous tab capture');
  }
  const streamId = await chrome.tabCapture.getMediaStreamId({
    targetTabId: tab.id,
  });
  const armResult = await chrome.runtime.sendMessage({
    target: 'offscreen',
    type: 'arm',
    streamId,
    title: tab.title || 'Selected tab',
  });
  if (!armResult?.ok) {
    throw new Error(armResult?.error || 'Unable to select this tab');
  }

  await chrome.action.setBadgeBackgroundColor({ color: '#2563eb' });
  await chrome.action.setBadgeText({ text: 'ARM' });
  await chrome.action.setTitle({ title: `Selected for Meetily: ${tab.title || 'Selected tab'}` });
  return armResult.status;
}

async function sendOffscreen(type) {
  await ensureOffscreenDocument();
  const result = await chrome.runtime.sendMessage({ target: 'offscreen', type });
  if (!result?.ok) throw new Error(result?.error || 'Capture command failed');
  return result.status;
}

async function handlePopupMessage(message) {
  if (message.type === 'arm_current') return armCurrentTab();
  if (message.type === 'status') return sendOffscreen('status');
  if (message.type === 'manual_start') return sendOffscreen('manual_start');
  if (message.type === 'stop_user') return sendOffscreen('stop_user');
  if (message.type === 'delete_user') return sendOffscreen('delete_user');
  throw new Error('Unknown capture command');
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.target === 'popup') {
    handlePopupMessage(message)
      .then((status) => sendResponse({ ok: true, status }))
      .catch(async (error) => {
        console.error('Meetily capture command failed', error);
        await chrome.action.setBadgeBackgroundColor({ color: '#dc2626' });
        await chrome.action.setBadgeText({ text: 'ERR' });
        sendResponse({ ok: false, error: error.message });
      });
    return true;
  }
  if (message.target !== 'service-worker') return;

  if (message.type === 'recording') {
    chrome.action.setBadgeBackgroundColor({ color: '#dc2626' });
    chrome.action.setBadgeText({ text: 'REC' });
  } else if (message.type === 'armed') {
    chrome.action.setBadgeBackgroundColor({ color: '#2563eb' });
    chrome.action.setBadgeText({ text: 'ARM' });
  } else if (message.type === 'disarmed') {
    chrome.action.setBadgeText({ text: '' });
    chrome.action.setTitle({ title: 'Open Meetily tab capture controls' });
  }
});
