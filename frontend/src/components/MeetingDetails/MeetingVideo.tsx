'use client';

import { convertFileSrc } from '@tauri-apps/api/core';
import { invoke } from '@tauri-apps/api/core';
import { Video } from 'lucide-react';
import { useEffect, useState } from 'react';

export function MeetingVideo({ folderPath }: { folderPath?: string | null }) {
  const [videoPath, setVideoPath] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    if (!folderPath) {
      setVideoPath(null);
      return;
    }
    invoke<string | null>('get_meeting_video_path', { folderPath })
      .then((path) => {
        if (active) setVideoPath(path);
      })
      .catch((error) => console.error('Failed to find meeting video', error));
    return () => {
      active = false;
    };
  }, [folderPath]);

  if (!videoPath) return null;

  return (
    <section className="border-b border-gray-200 bg-white p-3">
      <div className="mb-2 flex items-center gap-2 text-xs font-medium text-gray-700">
        <Video size={15} className="text-blue-600" />
        Meeting video
      </div>
      <video
        controls
        preload="metadata"
        className="aspect-video w-full rounded-lg bg-black"
        src={convertFileSrc(videoPath)}
      />
    </section>
  );
}
