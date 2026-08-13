import { notFound } from 'next/navigation';
import { ShowcaseShellPage } from '@/showcase/ShowcaseShellPage';

// `.showcase.tsx` is a page extension only under `next dev` (see next.config.js), so a
// production build never compiles this route at all. The guard stays as the second lock:
// it keeps the page unreachable if the extension is ever registered for a real build.
export default function ShowcaseRoute() {
  if (process.env.NODE_ENV !== 'development') notFound();
  return <ShowcaseShellPage />;
}
