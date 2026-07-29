import React from 'react';
import { Upload } from '@/components/deslop-icons';
import { getAudioFormatsDisplayList } from '@/constants/audioFormats';
import { useT } from '@/lib/i18n';

interface ImportDropOverlayProps {
  visible: boolean;
}

export function ImportDropOverlay({ visible }: ImportDropOverlayProps) {
  const t = useT();
  if (!visible) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm
                 flex items-center justify-center pointer-events-none
                 transition-opacity duration-200"
    >
      <div className="border-2 border-dashed border-primary/40 rounded-2xl
                      p-12 text-center bg-primary/10 shadow-none
                      transform scale-100 transition-transform">
        <Upload className="h-16 w-16 text-primary mx-auto mb-4" />
        <p className="text-xl font-medium text-primary-foreground">{t('Drop audio file to import')}</p>
        <p className="text-sm text-primary mt-2">{getAudioFormatsDisplayList()}</p>
      </div>
    </div>
  );
}
