import { memo } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

// Renders LLM responses as markdown. Raw HTML is disabled by default (safe).
// Memoised: during streaming every already-finished bubble would otherwise be
// re-parsed each time the growing reply re-renders its parent.
export const Markdown = memo(function Markdown({ children }: { children: string }) {
  return (
    <div className="markdown-body text-[13.5px] leading-[1.6]">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{children}</ReactMarkdown>
    </div>
  );
});
