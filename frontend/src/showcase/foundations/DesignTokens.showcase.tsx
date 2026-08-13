import { Button } from '@/components/ui/button';
import { Wordmark } from '@/components/memento/Wordmark';
import { Icon } from '@/components/memento/Icon';

export default function DesignTokensShowcase() {
  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-8 text-foreground">
      <div className="flex w-full max-w-xl flex-col gap-8 rounded-[var(--ui-radius-26)] bg-[var(--elevation-1)] p-[var(--ui-space-24)] shadow-[var(--shadow-2)]">
        <Wordmark />
        <div className="flex flex-wrap gap-[var(--ui-layout-control-gap)]">
          <Button><Icon name="mic" />Начать запись</Button>
          <Button variant="outline"><Icon name="calendar" />Календарь</Button>
          <Button variant="ghost"><Icon name="settings" />Настройки</Button>
        </div>
      </div>
    </main>
  );
}
