export function SettingsView() {
  return (
    <>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', marginBottom: 24 }}>
        <div><span className="font-mono text-[10px] tracking-[1px] text-muted">WORKSPACE</span><h1 style={{ margin: 0, fontSize: 24 }}>Settings</h1></div>
      </header>
      <div className="max-w-[700px] rounded-[9px] border border-line bg-panel p-[22px]">
        <h3 className="mb-[5px] text-[14px]">Theme</h3>
        <p className="mb-6 text-[12px] leading-[1.7] text-muted">Local-first agent workspace. Runs are recorded locally; agents are isolated per home directory.</p>
        <div className="mb-[31px] flex gap-[13px]">
          {(['dark', 'light', 'cyber'] as const).map((t) => (
            <button key={t} className={`w-[130px] cursor-pointer rounded-[7px] border border-line bg-transparent p-2 text-left hover:border-[#52525b] ${t === 'dark' ? 'border-[var(--purple)]' : ''}`} onClick={() => document.documentElement.setAttribute('data-theme', t)}>
              <span className={`mb-[7px] block h-[43px] rounded bg-[#f3f3f4] ${t === 'dark' ? 'bg-[#19191f]' : t === 'cyber' ? 'bg-[linear-gradient(135deg,#091020,#243267)]' : ''}`} />
              <b className="text-[11px]">{t}</b>
            </button>
          ))}
        </div>
        <p className="text-[12px] text-muted">Everything runs locally — your agents, tools, and data stay on this machine.</p>
      </div>
    </>
  );
}
