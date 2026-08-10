import { notFound } from 'next/navigation';
import { ShowcaseShellPage } from '@/showcase/ShowcaseShellPage';

export default function ShowcaseRoute() {
  if (process.env.NODE_ENV !== 'development') notFound();
  return <ShowcaseShellPage />;
}
