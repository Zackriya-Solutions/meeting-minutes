import { Button } from '@/components/ui/button';
import { SiriWave4 } from '@/components/ui/siri-wave-4';
import { useT } from '@/lib/i18n';

interface RecordOverlayProps {
  title?: string;
  onStop: () => void;
  meetingId?: string | null;
  isFinalizing?: boolean;
}

export function RecordOverlay({ onStop, isFinalizing = false }: RecordOverlayProps) {
  const t = useT();

  return (
    <section
      className="flex w-full min-w-[320px] flex-col gap-3"
      aria-label={t('Meeting recording')}
    >
      <SiriWave4
        active={!isFinalizing}
        processing={isFinalizing}
        height={64}
        sensitivity={1.55}
        className="text-foreground"
      />
      <Button
        type="button"
        size="lg"
        onClick={onStop}
        disabled={isFinalizing}
        aria-busy={isFinalizing}
        aria-label={isFinalizing ? t('Saving meeting…') : t('Finish')}
        className="w-full text-base disabled:opacity-100"
      >
        {isFinalizing ? (
          <>
            <span
              aria-hidden="true"
              className="block size-5 origin-center animate-spin rounded-full border-2 border-current border-r-transparent"
            />
            <span className="sr-only">{t('Saving meeting…')}</span>
          </>
        ) : t('Finish')}
      </Button>
    </section>
  );
}
