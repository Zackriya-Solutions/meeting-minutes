import React from 'react';
import { Download } from 'lucide-react';
import { toast } from 'sonner';
import { UpdateInfo } from '@/services/updateService';

let globalShowDialogCallback: (() => void) | null = null;

export function setUpdateDialogCallback(callback: () => void) {
  globalShowDialogCallback = callback;
}

export function showUpdateNotification(updateInfo: UpdateInfo, onUpdateClick?: () => void) {
  const handleClick = () => {
    if (onUpdateClick) {
      onUpdateClick();
    } else if (globalShowDialogCallback) {
      globalShowDialogCallback();
    }
  };

  // Use sonner's own title/description/action slots. The previous version
  // nested a hand-rolled flex row inside the toast body, which fought sonner's
  // layout and rendered as a clipped, doubled card.
  toast('Update available', {
    description: `Version ${updateInfo.version} is ready to install.`,
    icon: <Download className="h-4 w-4" />,
    duration: 10000,
    action: {
      label: 'View details',
      onClick: handleClick,
    },
  });
}
