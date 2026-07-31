import React from 'react';
import { Download } from '@/components/deslop-icons';
import { toast } from 'sonner';
import { UpdateInfo } from '@/services/updateService';
import { translate } from '@/lib/i18n';

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

  toast.info(
    <div className="flex items-center justify-between gap-4">
      <div className="flex items-center gap-2">
        <Download className="h-4 w-4" />
        <div>
          <p className="font-medium">{translate('Update Available')}</p>
          <p className="text-sm text-muted-foreground">
            {translate('Version')} {updateInfo.version} {translate('is now available')}
          </p>
        </div>
      </div>
      <button
        onClick={(e) => {
          e.stopPropagation();
          handleClick();
        }}
        className="text-sm font-medium text-primary hover:text-primary/90 underline"
      >
        {translate('View Details')}
      </button>
    </div>,
    {
      duration: 10000,
      position: 'bottom-center',
    }
  );
}
