import { useEffect, useRef, useState } from 'react';
import { Plus, RefreshCw, SquareTerminal, Trash2 } from 'lucide-react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import type { TerminalSession } from '../../runtime';
import { terminalStart, terminalWrite, terminalAttach, terminalResize, terminalList, terminalKill } from '../../runtime';
import { Button, IconButton } from '../ui/Button';
import { Card } from '../ui/Card';
import { FIELD_LABEL_CLS, INPUT_CLS } from '../ui/Input';
import { toast } from '../../hooks/useToast';

// Run real CLI tools as long-lived PTY sessions and drive them here. Each session
// can also be bound to a Telegram bot (Telegram → bots → target).
export function TerminalView() {
  const [sessions, setSessions] = useState<TerminalSession[]>([]);
  const [selected, setSelected] = useState('');
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [cwd, setCwd] = useState('');
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);

  const loadSessions = async () => {
    const list = await terminalList();
    setSessions(list);
    setSelected((cur) => (list.some((s) => s.id === cur) ? cur : (list[0]?.id ?? '')));
  };

  useEffect(() => {
    void loadSessions();
    const iv = setInterval(() => void loadSessions(), 2500);
    return () => clearInterval(iv);
  }, []);

  // One xterm per selected session; input -> pty, output <- attach channel.
  useEffect(() => {
    const host = hostRef.current;
    if (!host || !selected) return;
    const term = new Terminal({ convertEol: true, fontSize: 12, scrollback: 5000, theme: { background: '#0b0b0c', foreground: '#e5e5e5' } });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    try { fit.fit(); } catch { /* layout not ready */ }
    termRef.current = term;

    const dataSub = term.onData((d) => { void terminalWrite(selected, d); });
    const ro = new ResizeObserver(() => {
      try { fit.fit(); void terminalResize(selected, term.cols, term.rows); } catch { /* ignore */ }
    });
    ro.observe(host);
    void terminalAttach(selected, (s) => term.write(s));

    return () => { dataSub.dispose(); ro.disconnect(); term.dispose(); termRef.current = null; };
  }, [selected]);

  const start = async () => {
    if (!name.trim() || !command.trim()) { toast('name and command required', 'error'); return; }
    try {
      const { id } = await terminalStart(name.trim(), command.trim(), cwd.trim() || undefined);
      setName(''); setCommand(''); setCwd('');
      await loadSessions();
      setSelected(id);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <Card className="shrink-0">
        <div className="flex flex-wrap items-end gap-2">
          <div className="min-w-[140px] flex-1">
            <label className={FIELD_LABEL_CLS}>name</label>
            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. cmdc" className={INPUT_CLS} />
          </div>
          <div className="min-w-[220px] flex-[2]">
            <label className={FIELD_LABEL_CLS}>command</label>
            <input value={command} onChange={(e) => setCommand(e.target.value)} placeholder="e.g. cmdc  (runs in the shell)" className={INPUT_CLS} />
          </div>
          <div className="min-w-[140px] flex-1">
            <label className={FIELD_LABEL_CLS}>working directory</label>
            <input value={cwd} onChange={(e) => setCwd(e.target.value)} placeholder="optional" className={INPUT_CLS} />
          </div>
          <Button variant="primary" icon={<Plus size={12} />} onClick={() => void start()}>start</Button>
        </div>
      </Card>

      <div className="flex min-h-0 flex-1 gap-3">
        <div className="flex w-56 shrink-0 flex-col gap-1.5 overflow-y-auto">
          <div className="flex items-center justify-between">
            <span className="font-mono text-[10px] tracking-wider text-muted uppercase">sessions</span>
            <IconButton label="refresh" onClick={() => void loadSessions()}><RefreshCw size={12} /></IconButton>
          </div>
          {sessions.length === 0 && <p className="m-0 font-mono text-[10px] text-muted">no sessions — start one above</p>}
          {sessions.map((s) => (
            <button
              key={s.id}
              className={`focus-ring flex cursor-pointer items-center gap-2 rounded-lg border px-2.5 py-1.5 text-left transition-colors ${s.id === selected ? 'border-foreground/40 bg-background' : 'border-border bg-surface hover:border-foreground/30'}`}
              onClick={() => setSelected(s.id)}
            >
              <SquareTerminal size={12} className={s.alive ? 'text-foreground/70' : 'text-muted'} />
              <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{s.name}</span>
              <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${s.alive ? 'bg-foreground' : 'bg-muted'}`} />
              <span
                role="button"
                className="cursor-pointer text-muted hover:text-danger"
                onClick={(e) => { e.stopPropagation(); void terminalKill(s.id).then(loadSessions); }}
              ><Trash2 size={11} /></span>
            </button>
          ))}
        </div>

        <div className="min-h-0 flex-1 overflow-hidden rounded-xl border border-border bg-[#0b0b0c]">
          {selected
            ? <div ref={hostRef} className="h-full w-full p-1" />
            : <div className="grid h-full place-items-center font-mono text-[11px] text-muted">start a session to open a terminal</div>}
        </div>
      </div>
    </div>
  );
}
