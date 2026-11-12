// Trigger a file download from the webview.
//
// Kept in one place because the naive version — a detached anchor plus an
// immediate `URL.revokeObjectURL(url)` — silently does nothing in WebView2: the
// blob URL is read asynchronously once the click starts the download, so revoking
// it in the same tick aborts a download that hasn't begun. The anchor also has to
// be in the document for the click to count. Both mistakes fail quietly — the
// caller's "exported" toast still fires while no file is written.
export function saveFile(filename: string, data: string | Blob, mime = 'application/octet-stream'): void {
  const blob = data instanceof Blob ? data : new Blob([data], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.rel = 'noopener';
  a.style.display = 'none';
  document.body.appendChild(a);
  a.click();
  a.remove();
  // Release the blob once the webview has had time to start the download.
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
