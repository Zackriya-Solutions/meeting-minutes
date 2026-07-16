"use client";

import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, Save, Loader2, MessageSquare } from '@/components/memento/LucideCompat';
import Analytics from '@/lib/analytics';
import { useT } from '@/lib/i18n';

interface SummaryUpdaterButtonGroupProps {
  isSaving: boolean;
  isDirty: boolean;
  onSave: () => Promise<void>;
  onCopy: () => Promise<void>;
  onFind?: () => void;
  onOpenFolder: () => Promise<void>;
  onDiscuss: () => void;
  hasSummary: boolean;
}

export function SummaryUpdaterButtonGroup({
  isSaving,
  isDirty,
  onSave,
  onCopy,
  onFind,
  onOpenFolder,
  onDiscuss,
  hasSummary
}: SummaryUpdaterButtonGroupProps) {
  const t = useT();
  return (
    <ButtonGroup>
      {/* Save button */}
      <Button
        variant="outline"
        size="sm"
        className={`${isDirty ? 'bg-[color-mix(in_srgb,var(--success)_12%,transparent)]' : ""}`}
        title={
          isSaving
            ? t('Saving')
            : isDirty
              ? t('Save Changes')
              : t('The generated summary is saved automatically')
        }
        onClick={() => {
          Analytics.trackButtonClick('save_changes', 'meeting_details');
          onSave();
        }}
        disabled={isSaving || !isDirty}
      >
        {isSaving ? (
          <>
            <Loader2 className="animate-spin" />
            <span className="hidden lg:inline">{t('Saving...')}</span>
          </>
        ) : (
          <>
            <Save />
            <span className="hidden lg:inline">{t('Save')}</span>
          </>
        )}
      </Button>

      <Button
        variant="outline"
        size="sm"
        title={t('Discuss this meeting with AI')}
        onClick={onDiscuss}
        disabled={!hasSummary}
      >
        <MessageSquare />
        <span className="hidden lg:inline">{t('Discuss')}</span>
      </Button>

      {/* Copy button */}
      <Button
        variant="outline"
        size="sm"
        title={t('Copy Summary')}
        onClick={() => {
          Analytics.trackButtonClick('copy_summary', 'meeting_details');
          onCopy();
        }}
        disabled={!hasSummary}
        className="cursor-pointer"
      >
        <Copy />
        <span className="hidden lg:inline">{t('Copy')}</span>
      </Button>

      {/* Find button */}
      {/* {onFind && (
        <Button
          variant="outline"
          size="sm"
          title="Найти в сути"
          onClick={() => {
            Analytics.trackButtonClick('find_in_summary', 'meeting_details');
            onFind();
          }}
          disabled={!hasSummary}
          className="cursor-pointer"
        >
          <Search />
          <span className="hidden lg:inline">Найти</span>
        </Button>
      )} */}
    </ButtonGroup>
  );
}
