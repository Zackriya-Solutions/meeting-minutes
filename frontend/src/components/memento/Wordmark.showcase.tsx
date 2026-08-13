import { Wordmark } from './Wordmark';

export default function WordmarkShowcase() {
  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-8 text-foreground">
      <Wordmark width={240} />
    </main>
  );
}
