# Meetily browser tab capture

Unpacked Chrome/Edge extension for recording a selected browser tab with Meetily.

1. Open `chrome://extensions` in Chrome or `edge://extensions` in Edge.
2. Enable **Developer mode**.
3. Choose **Load unpacked** and select this directory.
4. Focus the tab to record and open the extension.
5. Choose **Select this tab**.
6. Either choose **Start** for a standalone capture, or select **Browser tab** in
   Meetily and begin a meeting recording.

The extension only connects to Meetily on `127.0.0.1`. It has no internet host
permissions and does not upload recordings. **Stop & save** finalizes the file;
**Delete recording** discards the active or most recently saved standalone
capture. Meeting recordings also stop and save automatically with the meeting.
