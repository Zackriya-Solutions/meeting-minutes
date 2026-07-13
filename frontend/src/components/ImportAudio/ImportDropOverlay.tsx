import React from 'react';
import { Upload } from '@/components/memento/LucideCompat';
import { getAudioFormatsDisplayList } from '@/constants/audioFormats';

interface ImportDropOverlayProps {
  visible: boolean;
}

export function ImportDropOverlay({ visible }: ImportDropOverlayProps) {
  if (!visible) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm
                 flex items-center justify-center pointer-events-none
                 transition-opacity duration-200"
    >
      <div className="border-2 border-dashed border-[var(--gold-border)] rounded-2xl
                      p-12 text-center bg-[var(--gold-soft)] shadow-none
                      transform scale-100 transition-transform">
        <Upload className="h-16 w-16 text-[var(--gold)] mx-auto mb-4" />
        <p className="text-xl font-medium text-[var(--fg-inverse)]">Drop audio file to import</p>
        <p className="text-sm text-[var(--gold)] mt-2">{getAudioFormatsDisplayList()}</p>
      </div>
    </div>
  );
}
