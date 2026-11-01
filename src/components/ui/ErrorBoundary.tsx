import { Component, type ErrorInfo, type ReactNode } from 'react';

// The shell mounts every view at once (hidden with CSS), so without a boundary a
// single view that throws during render unmounts the whole tree and blanks the
// window. Each pane is wrapped in this, so a broken view shows its error in place
// and the rest of the app keeps working.
export class ErrorBoundary extends Component<{ label: string; children: ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) { return { error }; }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`[${this.props.label}] view crashed:`, error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="grid h-full place-items-center p-6">
        <div className="max-w-[600px] rounded-xl border border-border bg-surface p-4">
          <b className="block font-mono text-[11px] lowercase text-danger">{this.props.label} failed to render</b>
          <p className="mt-2 mb-0 font-mono text-[10px] leading-relaxed break-words whitespace-pre-wrap text-muted">{error.message}</p>
          <p className="mt-2 mb-0 font-mono text-[10px] leading-relaxed break-words whitespace-pre-wrap text-muted">{String(error.stack ?? '').split('\n').slice(0, 5).join('\n')}</p>
        </div>
      </div>
    );
  }
}
