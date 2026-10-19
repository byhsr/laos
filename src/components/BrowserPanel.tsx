import { useCallback, useEffect, useRef, useState } from 'react';
import { ArrowLeft, ArrowRight, ExternalLink, RotateCw, X } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { IconButton } from './ui/Button';
import { Input } from './ui/Input';
import { useBrowserStore } from '../hooks/useBrowser';
import { browserBack, browserClose, browserForward, browserNavigate, browserOpen, browserReload, browserSetBounds } from '../runtime';

// The mini browser, docked to the LEFT edge so it slides in over the rail and
// sidebar while the chat stays visible on the right. The page itself is a real
// webview the Rust side paints over the empty viewport below the address bar —
// React positions it by reporting the viewport's rect (device pixels).
export function BrowserPanel({ hidden = false }: { hidden?: boolean }) {
  const url = useBrowserStore((s) => s.url);
  const loading = useBrowserStore((s) => s.loading);
  const closePanel = useBrowserStore((s) => s.closePanel);
  const viewportRef = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState(url);

  // The page can navigate itself (links, redirects) — keep the bar in step.
  useEffect(() => { setDraft(url); }, [url]);

  // Report the viewport rect to the backend; the webview is positioned there.
  const syncBounds = useCallback(() => {
    const el = viewportRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    void browserSetBounds(
      Math.round(r.left * dpr), Math.round(r.top * dpr),
      Math.round(r.width * dpr), Math.round(r.height * dpr),
    );
  }, []);

  // Keep the webview glued to the viewport as the layout changes: on mount, after
  // the slide-in animation, on resize, and whenever the viewport box changes.
  useEffect(() => {
    if (hidden) return;
    syncBounds();
    const after = setTimeout(syncBounds, 220);
    const ro = new ResizeObserver(syncBounds);
    if (viewportRef.current) ro.observe(viewportRef.current);
    window.addEventListener('resize', syncBounds);
    return () => { clearTimeout(after); ro.disconnect(); window.removeEventListener('resize', syncBounds); };
  }, [hidden, syncBounds]);

  // Show the webview while the drawer is up; hide it (but keep the page and its
  // login alive) when closed, or while another overlay is open on top.
  useEffect(() => {
    if (hidden) { void browserClose(); return; }
    void browserOpen();
    return () => { void browserClose(); };
  }, [hidden]);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!draft.trim()) return;
    void browserNavigate(draft.trim());
  };

  const openExternal = () => { if (url) void openUrl(url).catch(() => {}); };

  return (
    <div
      role="dialog"
      aria-label="in-app browser"
      className="app-no-drag browser-panel flex h-full min-h-0 w-[min(42vw,480px)] shrink-0 flex-col overflow-hidden border-r border-border bg-surface"
    >
      <header className="flex shrink-0 items-center gap-1.5 border-b border-border px-3 py-2">
        <span className="shrink-0 font-mono text-[10px] tracking-wider text-muted uppercase">in-app</span>
        <IconButton label="back" onClick={() => void browserBack()}><ArrowLeft size={14} /></IconButton>
        <IconButton label="forward" onClick={() => void browserForward()}><ArrowRight size={14} /></IconButton>
        <IconButton label="reload" onClick={() => void browserReload()}>
          <RotateCw size={14} className={loading ? 'animate-spin' : ''} />
        </IconButton>
        <form onSubmit={submit} className="min-w-0 flex-1">
          <Input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="search or enter address"
            aria-label="address"
            spellCheck={false}
          />
        </form>
        <IconButton label="open in system browser" onClick={openExternal}><ExternalLink size={14} /></IconButton>
        <IconButton label="close" onClick={() => { void browserClose(); closePanel(); }}><X size={14} /></IconButton>
      </header>

      {/* The Rust webview is painted over this box (a real page, not an iframe). */}
      <div ref={viewportRef} className="relative min-h-0 flex-1 bg-background">
        {!url && (
          <div className="grid h-full place-items-center p-6 text-center">
            <p className="m-0 max-w-[280px] font-mono text-[11px] leading-relaxed text-muted">
              the mini browser is opening — an agent that uses browser control will load pages here.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
