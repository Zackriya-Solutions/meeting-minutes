'use client';

import { useEffect } from 'react';
import { AlertTriangle, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';

export default function RouteError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    console.error('[App route] Unhandled route error', error);
  }, [error]);

  return (
    <section className="flex min-h-screen items-center justify-center bg-gray-50 p-8">
      <div className="w-full max-w-lg rounded-xl border border-red-200 bg-white p-6 shadow-sm">
        <div className="flex items-start gap-3">
          <AlertTriangle className="mt-0.5 h-6 w-6 shrink-0 text-red-600" />
          <div>
            <h1 className="text-lg font-semibold text-gray-900">No se pudo cargar esta página</h1>
            <p className="mt-1 text-sm text-gray-600">
              Reintentá la carga. El error fue registrado en la consola de desarrollo.
            </p>
            {process.env.NODE_ENV === 'development' && (
              <pre className="mt-3 max-h-40 overflow-auto rounded bg-gray-950 p-3 text-xs text-red-200">
                {error.message}
              </pre>
            )}
            <Button className="mt-4" onClick={reset}>
              <RefreshCw className="mr-2 h-4 w-4" />
              Reintentar
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}
