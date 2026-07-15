import React from 'react';
import { Upload } from 'lucide-react';
import { getAudioFormatsDisplayList } from '@/constants/audioFormats';
import { useTranslation } from 'react-i18next';

interface ImportDropOverlayProps {
  visible: boolean;
}

export function ImportDropOverlay({ visible }: ImportDropOverlayProps) {
  const { t } = useTranslation();
  if (!visible) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm
                 flex items-center justify-center pointer-events-none
                 transition-opacity duration-200"
    >
      <div className="border-2 border-dashed border-blue-400 rounded-2xl
                      p-12 text-center bg-blue-950/50 shadow-2xl
                      transform scale-100 transition-transform">
        <Upload className="h-16 w-16 text-blue-400 mx-auto mb-4" />
        <p className="text-xl font-medium text-white">{t('importAudio.dropAudioFile')}</p>
        <p className="text-sm text-blue-300 mt-2">{getAudioFormatsDisplayList()}</p>
      </div>
    </div>
  );
}
