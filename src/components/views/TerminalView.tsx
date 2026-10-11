import { useEffect, useRef, useState } from 'react';
import { Plus, RefreshCw, Settings2, SquareTerminal, Trash2, X } from 'lucide-react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import type { TerminalSession } from '../../runtime';
import { terminalStart, terminalWrite, terminalAttach, terminalResize, terminalList, terminalKill, getAppSettings, setDefaultShell, setCliShortcuts, type CliShortcut } from '../../runtime';
import { Button, IconButton } from '../ui/Button';
import { FIELD_LABEL_CLS, INPUT_CLS } from '../ui/Input';
import { toast } from '../../hooks/useToast';

// Run real CLI tools as long-lived PTY sessions and drive them here. Each session
// can also be bound to a Telegram bot (Telegram → bots → target).
export function TerminalView() {
  const [sessions, setSessions] = useState<TerminalSession[]>([]);
  const [selected, setSelected] = useState('');
  const [showConfig, setShowConfig] = useState(false);
  const [shell, setShell] = useState('');
  const [shortcuts, setShortcuts] = useState<CliShortcut[]>([]);
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

  useEffect(() => { getAppSettings().then((s) => { setShell(s.defaultShell); setShortcuts(s.cliShortcuts ?? []); }); }, []);

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

  // One click starts the default shell; type the tool (cmdc, …) inside it.
  const newSession = async () => {
    const cmd = shell.trim();
    if (!cmd) { toast('set a default shell in config', 'error'); return; }
    const n = sessions.filter((s) => s.name.startsWith(cmd)).length;
    try {
      const { id } = await terminalStart(n ? `${cmd} ${n + 1}` : cmd, cmd, undefined);
      await loadSessions();
      setSelected(id);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    }
  };

  const saveShell = async () => {
    try { await setDefaultShell(shell); toast('default shell saved', 'success'); }
    catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };

  const editShortcut = (i: number, key: keyof CliShortcut, value: string) =>
    setShortcuts((prev) => prev.map((s, idx) => (idx === i ? { ...s, [key]: value } : s)));
  const addShortcut = () => setShortcuts((prev) => [...prev, { name: '', command: '', cwd: '' }]);
  const removeShortcut = (i: number) => setShortcuts((prev) => prev.filter((_, idx) => idx !== i));
  const saveShortcuts = async () => {
    const clean = shortcuts.filter((s) => s.name.trim());
    setShortcuts(clean);
    try { await setCliShortcuts(clean); toast('cli shortcuts saved', 'success'); }
    catch (e) { toast(e instanceof Error ? e.message : String(e), 'error'); }
  };

  const openShortcut = async (s: CliShortcut) => {
    const cmd = s.command.trim() || shell.trim();
    if (!cmd) { toast('set a default shell or a command for this shortcut', 'error'); return; }
    try {
      const { id } = await terminalStart(s.name.trim() || cmd, cmd, s.cwd.trim() || undefined);
      await loadSessions();
      setSelected(id);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    }
  };

  return (
    <div className="flex h-full min-h-0 gap-3">
      <div className="flex min-h-0 flex-1 flex-col gap-3">
        <div className="flex min-h-0 flex-1 gap-3">
          <div className="flex w-56 shrink-0 flex-col gap-1.5 overflow-y-auto">
            <div className="flex items-center justify-between">
              <span className="font-mono text-[10px] tracking-wider text-muted uppercase">sessions</span>
              <div className="flex items-center gap-1">
                <IconButton label="new terminal" onClick={() => void newSession()}><Plus size={12} /></IconButton>
                <IconButton label="config" onClick={() => setShowConfig((v) => !v)}><Settings2 size={12} /></IconButton>
                <IconButton label="refresh" onClick={() => void loadSessions()}><RefreshCw size={12} /></IconButton>
              </div>
            </div>
            {sessions.length === 0 && <p className="m-0 font-mono text-[10px] text-muted">no sessions — hit + to start one</p>}
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

      {/* Side tray: default shell + `/cli` shortcuts. */}
      <aside className={`shrink-0 overflow-hidden transition-[width] duration-200 ${showConfig ? 'w-[360px]' : 'w-0'}`}>
        <div className="flex h-full w-[360px] flex-col gap-3 overflow-y-auto rounded-xl border border-border bg-surface p-3">
          <div className="flex items-center justify-between">
            <span className="font-mono text-[10px] tracking-wider text-muted uppercase">config</span>
            <IconButton label="close" onClick={() => setShowConfig(false)}><X size={12} /></IconButton>
          </div>

          <div>
            <label className={FIELD_LABEL_CLS}>default shell</label>
            <div className="flex items-end gap-2">
              <input
                list="shell-presets"
                value={shell}
                onChange={(e) => setShell(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') void saveShell(); }}
                placeholder="cmd"
                className={`${INPUT_CLS} min-w-0 flex-1`}
              />
              <Button variant="primary" onClick={() => void saveShell()}>save</Button>
            </div>
            <datalist id="shell-presets">
              <option value="cmd" />
              <option value="powershell" />
              <option value="pwsh" />
              <option value="bash" />
              <option value="wsl" />
              <option value="C:\Program Files\Git\bin\bash.exe" />
            </datalist>
          </div>

          <div className="border-t border-border pt-3">
            <span className="font-mono text-[10px] tracking-wider text-muted uppercase">cli shortcuts</span>
            <p className="mt-1 mb-2 text-[12px] leading-[1.65] text-muted">
              Named targets — `/cli &lt;name&gt;` on Telegram opens a session in that folder (empty command → the default shell).
            </p>
            <div className="grid gap-2">
              {shortcuts.map((s, i) => (
                <div key={i} className="rounded-lg border border-border p-2">
                  <div className="flex items-center gap-2">
                    <input value={s.name} onChange={(e) => editShortcut(i, 'name', e.target.value)} placeholder="name" className={`${INPUT_CLS} min-w-0 flex-1`} />
                    <IconButton label="remove" onClick={() => removeShortcut(i)}><Trash2 size={12} /></IconButton>
                  </div>
                  <input value={s.command} onChange={(e) => editShortcut(i, 'command', e.target.value)} placeholder="command (optional)" className={`${INPUT_CLS} mt-2`} />
                  <input value={s.cwd} onChange={(e) => editShortcut(i, 'cwd', e.target.value)} placeholder="folder / path" className={`${INPUT_CLS} mt-2`} />
                  <div className="mt-2 flex justify-end">
                    <Button icon={<SquareTerminal size={12} />} onClick={() => void openShortcut(s)}>open</Button>
                  </div>
                </div>
              ))}
            </div>
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <Button icon={<Plus size={12} />} onClick={addShortcut}>add</Button>
              <Button variant="primary" onClick={() => void saveShortcuts()}>save</Button>
            </div>
          </div>
        </div>
      </aside>
    </div>
  );
}
