import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

// Renders LLM responses as markdown. Raw HTML is disabled by default (safe).
export function Markdown({ children }: { children: string }) {
  return (
    <div className="markdown-body text-[13.5px] leading-[1.6]">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{children}</ReactMarkdown>
    </div>
  );
}
