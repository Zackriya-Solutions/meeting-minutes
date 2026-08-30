'use client';

import React from 'react';
import { AlertTriangle, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface MainContentErrorBoundaryProps {
  children: React.ReactNode;
}

interface MainContentErrorBoundaryState {
  error: Error | null;
}

// Keep navigation usable when a route subtree fails. Without this boundary a
// client rendering failure can leave the desktop shell looking like an empty
// white page while the fixed sidebar remains visible.
export class MainContentErrorBoundary extends React.Component<
  MainContentErrorBoundaryProps,
  MainContentErrorBoundaryState
> {
  state: MainContentErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): MainContentErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('[MainContent] Route render failed', error, errorInfo);
  }

  private retry = () => {
    this.setState({ error: null });
  };

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return (
      <section className="flex min-h-screen items-center justify-center bg-gray-50 p-8">
        <div className="w-full max-w-lg rounded-xl border border-red-200 bg-white p-6 shadow-sm">
          <div className="flex items-start gap-3">
            <AlertTriangle className="mt-0.5 h-6 w-6 shrink-0 text-red-600" />
            <div>
              <h1 className="text-lg font-semibold text-gray-900">No se pudo mostrar esta pantalla</h1>
              <p className="mt-1 text-sm text-gray-600">
                La navegación sigue disponible. Reintentá cargar esta vista; si el problema continúa,
                revisá la consola de desarrollo para ver el error registrado.
              </p>
              {process.env.NODE_ENV === 'development' && (
                <pre className="mt-3 max-h-40 overflow-auto rounded bg-gray-950 p-3 text-xs text-red-200">
                  {this.state.error.message}
                </pre>
              )}
              <Button className="mt-4" onClick={this.retry}>
                <RefreshCw className="mr-2 h-4 w-4" />
                Reintentar
              </Button>
            </div>
          </div>
        </div>
      </section>
    );
  }
}
