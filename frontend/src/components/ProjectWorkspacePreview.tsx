'use client';

import { useState } from 'react';
import { FileText, FolderKanban, Plus, ShieldCheck } from 'lucide-react';

const starterProjects = [
  {
    id: 'product-feedback',
    name: 'Product feedback',
    description: 'Keep customer calls, decisions, and follow-ups together.',
    captures: 0,
  },
  {
    id: 'weekly-planning',
    name: 'Weekly planning',
    description: 'Collect planning notes before they disappear into separate documents.',
    captures: 0,
  },
];

export function ProjectWorkspacePreview() {
  const [projects, setProjects] = useState(starterProjects);

  const addLocalProject = () => {
    const nextNumber = projects.length + 1;
    setProjects((current) => [
      ...current,
      {
        id: `local-project-${nextNumber}`,
        name: `Untitled project ${nextNumber}`,
        description: 'A temporary project for exploring this workspace.',
        captures: 0,
      },
    ]);
  };

  return (
    <section aria-labelledby="project-preview-title" className="space-y-5">
      <div className="flex flex-col gap-4 border-b border-[var(--pt-border)] pb-5 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-2 flex items-center gap-2 text-[var(--pt-warning)]">
            <ShieldCheck className="h-4 w-4" aria-hidden="true" />
            <span className="pt-label">Local preview</span>
          </div>
          <h2 id="project-preview-title" className="text-[21px] font-medium tracking-[-0.025em] text-[var(--pt-text)]">
            Try the project workspace
          </h2>
          <p className="mt-2 max-w-2xl text-sm text-[var(--pt-text-secondary)]">
            Changes on this screen last only until you close or refresh the app. Project storage is not connected yet.
          </p>
        </div>
        <button
          type="button"
          onClick={addLocalProject}
          className="inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-[3px] border border-transparent bg-[var(--pt-accent)] px-4 text-sm font-medium text-[var(--pt-text)] transition-colors hover:bg-[var(--pt-accent-hover)] active:bg-[var(--pt-accent-active)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--pt-accent)]"
        >
          <Plus className="h-4 w-4" aria-hidden="true" />
          Add preview project
        </button>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        {projects.map((project) => (
          <article key={project.id} className="rounded-[3px] border border-[var(--pt-border)] bg-[var(--pt-surface)] p-5 shadow-sm transition-[transform,border-color,box-shadow] hover:-translate-y-0.5 hover:border-[var(--pt-border-strong)] hover:shadow-md">
            <div className="flex items-start gap-4">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center border border-[var(--pt-border)] bg-[var(--pt-surface-alt)]">
                <FolderKanban className="h-5 w-5 text-[var(--pt-text-secondary)]" aria-hidden="true" />
              </div>
              <div className="min-w-0 flex-1">
                <h3 className="text-base font-medium text-[var(--pt-text)]">{project.name}</h3>
                <p className="mt-1 text-sm leading-6 text-[var(--pt-text-secondary)]">{project.description}</p>
                <div className="mt-4 flex items-center gap-2 text-xs font-medium text-[var(--pt-text-tertiary)]">
                  <FileText className="h-4 w-4" aria-hidden="true" />
                  {project.captures} captures
                </div>
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
