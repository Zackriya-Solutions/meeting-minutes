import Link from 'next/link';
import { ArrowRight, Inbox, Mic } from 'lucide-react';

export default function InboxPage() {
  return (
    <div className="h-full overflow-y-auto bg-[var(--pt-bg)] px-5 py-8 md:px-8">
      <main className="mx-auto max-w-[1180px]">
        <header className="mb-8 max-w-3xl">
          <p className="pt-label mb-3 text-[var(--pt-accent-active)]">Capture queue</p>
          <h1 className="text-[30px] font-medium leading-[1.1] tracking-[-0.04em] text-[var(--pt-text)]">Inbox</h1>
          <p className="mt-3 font-[var(--pt-font-reading)] text-[17px] leading-7 text-[var(--pt-text-secondary)]">
            Review new recordings before you file them into a project. Your saved meetings remain available in the sidebar.
          </p>
        </header>

        <section aria-labelledby="inbox-empty-title" className="max-w-3xl rounded-[3px] border border-[var(--pt-border)] bg-[var(--pt-surface)] p-6 shadow-sm md:p-8">
          <div className="flex h-11 w-11 items-center justify-center border border-[var(--pt-border)] bg-[var(--pt-surface-alt)]">
            <Inbox className="h-5 w-5 text-[var(--pt-text-secondary)]" aria-hidden="true" />
          </div>
          <h2 id="inbox-empty-title" className="mt-6 text-[21px] font-medium tracking-[-0.025em] text-[var(--pt-text)]">
            New captures will land here
          </h2>
          <p className="mt-2 max-w-xl text-sm leading-6 text-[var(--pt-text-secondary)]">
            Inbox assignment is not connected to storage yet. For now, use the meeting list in the sidebar to open saved recordings.
          </p>
          <Link
            href="/?mode=record-meeting"
            className="mt-6 inline-flex min-h-10 w-fit items-center justify-center gap-2 rounded-[3px] border border-transparent bg-[var(--pt-accent)] px-4 text-sm font-medium text-[var(--pt-text)] transition-colors hover:bg-[var(--pt-accent-hover)] active:bg-[var(--pt-accent-active)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--pt-accent)]"
          >
            <Mic className="h-4 w-4" aria-hidden="true" />
            New capture
            <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </Link>
        </section>
      </main>
    </div>
  );
}
