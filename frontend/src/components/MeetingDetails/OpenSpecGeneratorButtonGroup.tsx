"use client";

import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { FileCode2, Loader2, RotateCcw } from 'lucide-react';
import { useI18n } from '@/hooks/useI18n';

type OpenSpecStatus = 'idle' | 'generating' | 'done' | 'error';

interface OpenSpecGeneratorButtonGroupProps {
  hasTranscripts?: boolean;
  status: OpenSpecStatus;
  onGenerate: () => Promise<void>;
  onRegenerate: () => Promise<void>;
}

interface OpenSpecGeneratorButtonGroupViewProps extends OpenSpecGeneratorButtonGroupProps {
  t: (key: string) => string;
}

export function OpenSpecGeneratorButtonGroupView({
  hasTranscripts = true,
  status,
  onGenerate,
  onRegenerate,
  t,
}: OpenSpecGeneratorButtonGroupViewProps) {
  if (!hasTranscripts) {
    return null;
  }

  const isGenerating = status === 'generating';
  const isDone = status === 'done';

  return (
    <ButtonGroup>
      <Button
        variant="outline"
        size="sm"
        className="bg-gradient-to-r from-emerald-50 to-cyan-50 hover:from-emerald-100 hover:to-cyan-100 border-emerald-200 xl:px-4"
        onClick={() => {
          if (isDone) {
            void onRegenerate();
            return;
          }
          void onGenerate();
        }}
        disabled={isGenerating}
        title={isDone ? t('openspec.regenerate') : t('openspec.generate')}
      >
        {isGenerating ? (
          <>
            <Loader2 className="animate-spin xl:mr-2" size={18} />
            <span className="hidden lg:inline xl:inline">{t('openspec.generating')}</span>
          </>
        ) : isDone ? (
          <>
            <RotateCcw className="xl:mr-2" size={18} />
            <span className="hidden lg:inline xl:inline">{t('openspec.regenerate')}</span>
          </>
        ) : (
          <>
            <FileCode2 className="xl:mr-2" size={18} />
            <span className="hidden lg:inline xl:inline">{t('openspec.generate')}</span>
          </>
        )}
      </Button>
    </ButtonGroup>
  );
}

export function OpenSpecGeneratorButtonGroup({
  hasTranscripts = true,
  status,
  onGenerate,
  onRegenerate,
}: OpenSpecGeneratorButtonGroupProps) {
  const { t } = useI18n();

  return OpenSpecGeneratorButtonGroupView({
    hasTranscripts,
    status,
    onGenerate,
    onRegenerate,
    t,
  });
}
