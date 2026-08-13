import { notFound } from 'next/navigation';
import { Suspense } from 'react';
import { ShowcasePreviewPage } from '@/showcase/ShowcasePreviewPage';

// Dev-only route: `.showcase.tsx` counts as a page extension only under `next dev`
// (see next.config.js), and the guard below keeps it unreachable either way.
export default function ShowcasePreviewRoute() {
  if (process.env.NODE_ENV !== 'development') notFound();
  return (
    <Suspense fallback={null}>
      <ShowcasePreviewPage />
    </Suspense>
  );
}
