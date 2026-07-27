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
      justification: 'Record the exact tab selected by the user for a Meetily meeting',
    });
  }
}

chrome.action.onClicked.addListener(async (tab) => {
  if (!tab.id) return;

  try {
    await ensureOffscreenDocument();
    const streamId = await chrome.tabCapture.getMediaStreamId({
      targetTabId: tab.id,
    });
    await chrome.runtime.sendMessage({
      target: 'offscreen',
      type: 'arm',
      streamId,
      title: tab.title || 'Selected tab',
    });
    await chrome.action.setBadgeBackgroundColor({ color: '#2563eb' });
    await chrome.action.setBadgeText({ text: 'ARM' });
    await chrome.action.setTitle({ title: `Armed for Meetily: ${tab.title || 'Selected tab'}` });
  } catch (error) {
    console.error('Unable to arm tab capture', error);
    await chrome.action.setBadgeBackgroundColor({ color: '#dc2626' });
    await chrome.action.setBadgeText({ text: 'ERR' });
  }
});

chrome.runtime.onMessage.addListener((message) => {
  if (message.target !== 'service-worker') return;
  if (message.type === 'recording') {
    chrome.action.setBadgeBackgroundColor({ color: '#dc2626' });
    chrome.action.setBadgeText({ text: 'REC' });
  } else if (message.type === 'armed') {
    chrome.action.setBadgeBackgroundColor({ color: '#2563eb' });
    chrome.action.setBadgeText({ text: 'ARM' });
  } else if (message.type === 'disarmed') {
    chrome.action.setBadgeText({ text: '' });
    chrome.action.setTitle({ title: 'Arm this tab for Meetily' });
  }
});
