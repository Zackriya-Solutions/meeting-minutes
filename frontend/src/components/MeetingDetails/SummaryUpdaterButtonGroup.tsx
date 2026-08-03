"use client";

import { Button } from '@/components/ui/fluid-button';
import { MaterialSymbol } from '@/vendor/deslop/primitives/material-symbols-react';
import Analytics from '@/lib/analytics';
import { useT } from '@/lib/i18n';

type FluidIconProps = { size?: number; strokeWidth?: number; className?: string };
const SaveIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="save" size={size} weight={400} className={className} />;
const DiscussIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="chat" size={size} weight={400} className={className} />;
const CopyIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="content_copy" size={size} weight={400} className={className} />;

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
    <div className="flex w-fit items-center gap-2">
      {/* Save button */}
      <Button
        variant="ghost"
        size="md"
        leadingIcon={SaveIcon}
        loading={isSaving}
        className={`${isDirty ? 'bg-success/10' : ""}`}
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
        {isSaving ? t('Saving...') : t('Save')}
      </Button>

      <Button
        variant="ghost"
        size="md"
        leadingIcon={DiscussIcon}
        title={t('Discuss this meeting with AI')}
        onClick={onDiscuss}
        disabled={!hasSummary}
      >
        {t('Discuss')}
      </Button>

      {/* Copy button */}
      <Button
        variant="ghost"
        size="md"
        leadingIcon={CopyIcon}
        title={t('Copy Summary')}
        onClick={() => {
          Analytics.trackButtonClick('copy_summary', 'meeting_details');
          onCopy();
        }}
        disabled={!hasSummary}
      >
        {t('Copy')}
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
    </div>
  );
}
