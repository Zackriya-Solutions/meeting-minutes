import React, { useEffect, useState } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
import * as ui from './ui';

// VALUEOS: first configuration step — choose the local transcript folder. Must be set and
// writable before capture is possible (Continue stays disabled until then).
export function ConfigScreen({ onDone }: { onDone: () => void }) {
  const { config } = useValueOs();
  const [folder, setFolder] = useState<string | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    config.getTranscriptFolder().then(setFolder);
  }, [config]);

  const choose = async () => {
    setError('');
    const picked = await config.pickFolder();
    if (!picked) return;
    const ok = await config.validateWritable(picked);
    if (!ok) {
      setError("That folder isn't writable — choose another.");
      return;
    }
    await config.setTranscriptFolder(picked);
    setFolder(picked);
  };

  return (
    <div data-testid="valueos-config" style={ui.page}>
      <div style={ui.card}>
        <h1 style={ui.h1}>Where should transcripts be saved?</h1>
        <p style={ui.sub}>
          Pick a local folder. Transcripts are stored here on this device before they are
          uploaded to ValueOS.
        </p>
        <button data-testid="valueos-config-pick" style={ui.ghostBtn} onClick={choose}>
          Choose folder…
        </button>
        {folder && (
          <p data-testid="valueos-config-folder" style={{ ...ui.sub, marginTop: 16 }}>
            {folder}
          </p>
        )}
        {error && (
          <p data-testid="valueos-config-error" style={{ ...ui.sub, color: '#ffd7d7' }}>
            {error}
          </p>
        )}
        <button
          data-testid="valueos-config-continue"
          style={folder ? ui.primaryBtn : ui.primaryBtnDisabled}
          disabled={!folder}
          onClick={() => folder && onDone()}
        >
          Continue
        </button>
      </div>
      <footer style={ui.footer}>Value Accelerator GmbH</footer>
    </div>
  );
}
