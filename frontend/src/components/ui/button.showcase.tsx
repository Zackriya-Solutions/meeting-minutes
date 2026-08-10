import { Button } from './button';
import { Icon } from '@/components/memento/Icon';

export default function ButtonShowcase() {
  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-8 text-foreground">
      <div className="flex flex-wrap items-center gap-3">
        <Button><Icon name="mic" />Записать встречу</Button>
        <Button variant="outline">Открыть архив</Button>
        <Button variant="ghost">Отмена</Button>
        <Button disabled>Сохранение…</Button>
      </div>
    </main>
  );
}
