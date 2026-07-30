import { Button } from '@/components/ui/button';
import { LiveWaveform } from '@/components/ui/live-waveform';
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
      <LiveWaveform
        active={!isFinalizing}
        mode="static"
        height={64}
        barWidth={3}
        barGap={2}
        barRadius={2}
        fadeWidth={32}
        sensitivity={1.35}
        smoothingTimeConstant={0.85}
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
