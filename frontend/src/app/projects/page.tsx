import { ProjectWorkspacePreview } from '@/components/ProjectWorkspacePreview';

export default function ProjectsPage() {
  return (
    <div className="h-full overflow-y-auto bg-[var(--pt-bg)] px-5 py-8 md:px-8">
      <main className="mx-auto max-w-[1180px]">
        <header className="mb-8 max-w-3xl">
          <p className="pt-label mb-3 text-[var(--pt-accent-active)]">Context workspace</p>
          <h1 className="text-[30px] font-medium leading-[1.1] tracking-[-0.04em] text-[var(--pt-text)]">Projects</h1>
          <p className="mt-3 font-[var(--pt-font-reading)] text-[17px] leading-7 text-[var(--pt-text-secondary)]">
            Group recordings, transcripts, and working context around the outcome you need to move forward.
          </p>
        </header>

        <ProjectWorkspacePreview />
      </main>
    </div>
  );
}
