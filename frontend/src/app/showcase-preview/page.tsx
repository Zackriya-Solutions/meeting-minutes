import { notFound } from 'next/navigation';
import { Suspense } from 'react';
import { ShowcasePreviewPage } from '@/showcase/ShowcasePreviewPage';

export default function ShowcasePreviewRoute() {
  if (process.env.NODE_ENV !== 'development') notFound();
  return (
    <Suspense fallback={null}>
      <ShowcasePreviewPage />
    </Suspense>
  );
}
