import { useState, type ComponentType } from 'react';
import { Cpu, Sparkles, Zap } from 'lucide-react';
import {
  SiAnthropic, SiGooglegemini, SiHuggingface, SiMeta, SiMistralai,
  SiNvidia, SiOllama, SiOpenrouter, SiPerplexity, SiX,
} from 'react-icons/si';
import type { ModelConfig } from '../../types';
import { useProvidersStore } from '../../hooks/useProviders';

type Mark = ComponentType<{ size?: number; className?: string }>;

// Provider marks. Simple Icons has no OpenAI or Groq glyph, so those fall through
// to a signalling shape rather than a wrong logo — a wrong logo is worse than a
// neutral one.
const BRAND: Record<string, Mark> = {
  ollama: SiOllama,
  anthropic: SiAnthropic,
  google: SiGooglegemini,
  openrouter: SiOpenrouter,
  xai: SiX,
};
const GLYPH: Record<string, Mark> = {
  openai: Sparkles,
  groq: Zap,
};
// OpenRouter proxies many vendors, so its id names the author before the slash
// (`anthropic/claude-…`, `meta-llama/…`). Show that author's mark instead of the
// router for a model that has one.
const AUTHOR: Record<string, Mark> = {
  anthropic: SiAnthropic,
  google: SiGooglegemini,
  meta: SiMeta,
  'meta-llama': SiMeta,
  mistralai: SiMistralai,
  nvidia: SiNvidia,
  perplexity: SiPerplexity,
  huggingface: SiHuggingface,
};

function brandFor(model: Pick<ModelConfig, 'provider' | 'model'>): Mark {
  if (model.provider === 'openrouter') {
    const author = model.model.split('/')[0]?.toLowerCase() ?? '';
    const known = AUTHOR[author];
    if (known) return known;
  }
  return BRAND[model.provider] ?? GLYPH[model.provider] ?? Cpu;
}

// A self-hosted model server (Ollama on a remote host) is best identified by its
// own site icon, so fetch that when the host is a real domain. Localhost has no
// icon to fetch, so those fall back to the brand mark.
function faviconDomain(host?: string): string | null {
  if (!host) return null;
  try {
    const url = new URL(host.includes('://') ? host : `http://${host}`);
    const name = url.hostname;
    if (!name || ['localhost', '127.0.0.1', '0.0.0.0', '::1'].includes(name)) return null;
    return name;
  } catch {
    return null;
  }
}

export function ModelLogo({ model, size = 14, className }: {
  model: Pick<ModelConfig, 'provider' | 'host' | 'model'>;
  size?: number;
  className?: string;
}) {
  const [broken, setBroken] = useState(false);
  // A custom provider has no brand mark, so fall back to its endpoint's favicon.
  const baseUrl = useProvidersStore((s) => s.providers.find((p) => p.id === model.provider)?.baseUrl);
  const hasBrand = !!(BRAND[model.provider] ?? GLYPH[model.provider]);
  const domain = (model.provider === 'ollama' || !hasBrand)
    ? faviconDomain(model.provider === 'ollama' ? model.host : baseUrl)
    : null;

  if (domain && !broken) {
    return (
      <img
        src={`https://www.google.com/s2/favicons?sz=${Math.min(128, size * 4)}&domain=${encodeURIComponent(domain)}`}
        width={size}
        height={size}
        alt=""
        className={`shrink-0 rounded-[3px] ${className ?? ''}`}
        onError={() => setBroken(true)}
      />
    );
  }

  const Brand = brandFor(model);
  return <Brand size={size} className={`shrink-0 ${className ?? ''}`} aria-hidden />;
}
