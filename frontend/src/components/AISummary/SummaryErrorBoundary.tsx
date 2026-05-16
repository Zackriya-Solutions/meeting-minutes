"use client";

import { Component, ReactNode } from "react";

interface Props {
  /** Raw markdown to show in the fallback when rendering fails. */
  rawMarkdown?: string;
  /** Optional retry handler (e.g. trigger a regenerate). */
  onRetry?: () => void;
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Catches render-phase errors thrown by the summary view (e.g. BlockNote
 * choking on otherwise-valid block input). Without this, a bad summary
 * takes down the entire meeting page with Next.js's generic
 * "Application error: a client-side exception has occurred" overlay.
 *
 * The fallback shows the raw markdown so the user still has access to
 * the summary content and can decide whether to regenerate.
 */
export class SummaryErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack?: string }) {
    console.error("SummaryErrorBoundary caught:", error, info.componentStack);
  }

  private handleRetry = () => {
    this.setState({ error: null });
    this.props.onRetry?.();
  };

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return (
      <div className="flex flex-col gap-3 p-4 border border-amber-300 bg-amber-50 rounded">
        <div className="text-sm text-amber-900">
          <strong>Couldn&apos;t render this summary.</strong> The model output
          contained something the editor couldn&apos;t parse. The raw text is
          shown below.
        </div>
        {this.props.rawMarkdown ? (
          <pre className="text-xs whitespace-pre-wrap bg-white p-3 rounded border border-amber-200 max-h-96 overflow-auto">
            {this.props.rawMarkdown}
          </pre>
        ) : null}
        {this.props.onRetry ? (
          <button
            type="button"
            onClick={this.handleRetry}
            className="self-start text-sm px-3 py-1 rounded bg-amber-600 text-white hover:bg-amber-700"
          >
            Regenerate summary
          </button>
        ) : null}
      </div>
    );
  }
}
