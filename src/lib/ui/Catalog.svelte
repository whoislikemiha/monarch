<script lang="ts">
  // Living catalog for the Monarch visual-language system.
  // Mounted standalone via `?catalog` (see main.ts) — replaces the app,
  // so its global atoms.css import never touches the legacy UI.
  // Renders every token group + component atom, and drives the real
  // theme system so themes are verified, not mocked.
  import "./styles/atoms.css";
  import { applyTheme, listThemes, DEFAULT_THEME, type ThemeId } from "../themes";
  import EventIcon from "./EventIcon.svelte";

  let theme = $state<ThemeId>(DEFAULT_THEME);
  const themeList = listThemes();

  function setTheme(id: ThemeId) {
    theme = applyTheme(id);
  }
  // Apply the default on mount so the catalog matches the app.
  setTheme(DEFAULT_THEME);

  let treeOpen = $state(true);
  let codeOpen = $state(false);

  const grades = ["E", "D", "C", "B", "A", "S"] as const;
  // MON-124 event taxonomy — kind · label · mono note.
  const eventKinds = [
    { k: "action", n: "coherent action", d: "headline · weighted" },
    { k: "tool", n: "tool call", d: "nested" },
    { k: "outcome", n: "action outcome", d: "closes a card" },
    { k: "decision", n: "decision", d: "fork — never ◇" },
    { k: "plan", n: "plan event", d: "set / step" },
    { k: "status", n: "status change", d: "divider" },
    { k: "note", n: "note", d: "manual" },
    { k: "blocker", n: "blocker", d: "warning tone" },
    { k: "question", n: "question", d: "manual" },
    { k: "chat", n: "chat spawned", d: "artifact" },
    { k: "report", n: "report", d: "objective close" },
    { k: "event", n: "fallback", d: "unknown kind" },
  ] as const;
  const elevations = [
    { v: "--bg-sink", u: "sidebar · deepest recesses" },
    { v: "--bg-base", u: "app canvas" },
    { v: "--bg-panel", u: "panels, cards, list rows" },
    { v: "--bg-raised", u: "inputs, hover, raised rows" },
    { v: "--bg-overlay", u: "selected/active, menus, tooltips" },
  ];
</script>

<div class="cat">
  <header class="masthead">
    <div class="brand">
      <div class="crown" aria-hidden="true"></div>
      <div>
        <div class="name">Monarch</div>
        <div class="sub mono">Visual Language · live</div>
      </div>
    </div>
    <div class="themebar" role="group" aria-label="Theme">
      {#each themeList as t}
        <button aria-pressed={theme === t.id} onclick={() => setTheme(t.id)}>{t.label}</button>
      {/each}
    </div>
  </header>

  <main>
    <!-- PALETTE -->
    <section class="section">
      <div class="section-head"><span class="idx mono">01</span><h2>Palette</h2></div>

      <div class="glabel"><span class="t">Elevation ladder</span><span class="rule"></span></div>
      <div class="elev-stack">
        {#each elevations as e}
          <div style="background:var({e.v})">
            <div class="nm mono">{e.v}</div>
            <div class="hx">{e.u}</div>
          </div>
        {/each}
      </div>

      <div class="glabel"><span class="t">Accents · status</span><span class="rule"></span></div>
      <div class="chiprow">
        {#each [["--accent", "primary"], ["--accent-2", "secondary"], ["--status-success", "success"], ["--status-warning", "warning"], ["--status-error", "error"], ["--status-info", "info"]] as [v, label]}
          <div class="csample"><span class="sw-dot" style="background:var({v})"></span><span class="lab mono">{v}<small>{label}</small></span></div>
        {/each}
      </div>

      <div class="glabel"><span class="t">Grade ramp · E → S</span><span class="rule"></span></div>
      <div class="grade-ramp">
        {#each grades as g}
          <div class="grade-col">
            <div class="grade-badge" style="color:var(--grade-{g.toLowerCase()});border-color:var(--grade-{g.toLowerCase()});background:color-mix(in srgb, var(--grade-{g.toLowerCase()}) 12%, var(--bg-panel))">{g}</div>
            <span class="gx mono">--grade-{g.toLowerCase()}</span>
          </div>
        {/each}
      </div>
    </section>

    <!-- TYPOGRAPHY -->
    <section class="section">
      <div class="section-head"><span class="idx mono">02</span><h2>Typography</h2></div>
      <div class="text-spec">
        <div class="row"><span style="font-size:32px;font-weight:700;letter-spacing:-0.02em;color:var(--text-primary)">Captain's Bridge</span><span class="meta mono">Inter 700 · 32</span></div>
        <div class="row"><span style="font-size:23px;font-weight:600;color:var(--text-primary)">Active Objectives</span><span class="meta mono">Inter 600 · 23</span></div>
        <div class="row"><span style="font-size:13px;color:var(--text-secondary);max-width:48ch">Body copy is Inter at 13px — comfortable for long reading-heavy sessions at the console.</span><span class="meta mono">Inter 400 · 13</span></div>
        <div class="row"><span class="mono" style="font-size:12px;color:var(--accent-2)">SH-0271 · grade A · 64% distilled</span><span class="meta mono">Mono · ids · metrics</span></div>
      </div>
      <p class="note">Inter for everything a human reads as language. JetBrains Mono only for ids, metrics, paths, timestamps, code.</p>
    </section>

    <!-- SPACING / RADIUS -->
    <section class="section">
      <div class="section-head"><span class="idx mono">03</span><h2>Spacing &amp; Radius</h2></div>
      <div class="glabel"><span class="t">Spacing · 4px base</span><span class="rule"></span></div>
      <div class="ruler">
        {#each [1, 2, 3, 4, 5, 6, 7, 8] as n}
          <div class="r"><div class="bar" style="width:var(--s{n})"></div><span class="lab mono">--s{n}</span></div>
        {/each}
      </div>
      <div class="glabel"><span class="t">Radius · small &amp; uniform</span><span class="rule"></span></div>
      <div class="radius-spec">
        {#each [["--r-sm", "2px"], ["--r-md", "4px"], ["--r-lg", "6px"]] as [v, px]}
          <figure><div class="rbox" style="border-radius:var({v})"></div><figcaption class="mono">{v}<b>{px}</b></figcaption></figure>
        {/each}
      </div>
      <p class="note">No shadows, glows, or blurs anywhere — depth is elevation + 1px border + space.</p>
    </section>

    <!-- COMPONENTS -->
    <section class="section">
      <div class="section-head"><span class="idx mono">04</span><h2>Component Grammar</h2></div>
      <div class="grid2">
        <!-- buttons -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Buttons</h4></div><span class="cap">action</span></div>
          <div class="panel-body">
            <div class="btnrow">
              <button class="btn btn-primary">Summon shadow</button>
              <button class="btn btn-ghost">Cancel</button>
              <button class="btn btn-danger">Dismiss</button>
            </div>
          </div>
        </div>

        <!-- inputs -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Inputs</h4></div><span class="cap">form</span></div>
          <div class="panel-body">
            <div class="field"><label for="ci">Objective title</label><input id="ci" class="input" placeholder="e.g. Refactor the Keeper queue" /></div>
            <div class="field"><label for="cs">Grade floor</label><select id="cs" class="select"><option>A — Vanguard</option><option>S — Sovereign</option></select></div>
          </div>
        </div>

        <!-- badges, chips, grades -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Badges · Chips · Grades</h4></div><span class="cap">status</span></div>
          <div class="panel-body">
            <div class="btnrow">
              <span class="badge b-success"><span class="bdot"></span>Complete</span>
              <span class="badge b-info"><span class="bdot"></span>Running</span>
              <span class="badge b-warning"><span class="bdot"></span>Stalled</span>
              <span class="badge b-error"><span class="bdot"></span>Failed</span>
            </div>
            <div class="btnrow">
              {#each grades as g}
                <span class="gchip" style="color:var(--grade-{g.toLowerCase()});border-color:var(--grade-{g.toLowerCase()});background:color-mix(in srgb, var(--grade-{g.toLowerCase()}) 14%, transparent)">{g}</span>
              {/each}
            </div>
            <div class="btnrow">
              <span class="chip">memory <span class="mono">·12</span></span>
              <span class="chip chip-scope">scope: repo</span>
              <span class="chip">objective <span class="mono">#O-118</span></span>
            </div>
          </div>
        </div>

        <!-- status dots + avatars -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Status &amp; Shadows</h4></div><span class="cap">presence</span></div>
          <div class="panel-body">
            <div class="dotlist">
              <div class="d"><span class="sdot success"></span>Idle · ready<span class="mono">circle</span></div>
              <div class="d"><span class="sdot running"></span>Running · on objective<span class="mono">pulse</span></div>
              <div class="d"><span class="sdot warning"></span>Stalled · needs input<span class="mono">diamond</span></div>
              <div class="d"><span class="sdot error"></span>Failed<span class="mono">triangle</span></div>
            </div>
            <div class="avatars">
              <div class="avatar ring" style="--gc:var(--grade-s)">ON<span class="pip" style="background:var(--status-info)"></span></div>
              <div class="avatar ring" style="--gc:var(--grade-a)">VE<span class="pip" style="background:var(--status-success)"></span></div>
              <div class="avatar ring" style="--gc:var(--grade-c)">WR<span class="pip" style="background:var(--status-warning)"></span></div>
            </div>
          </div>
        </div>

        <!-- meter -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Meters</h4></div><span class="cap">progress</span></div>
          <div class="panel-body">
            <div class="meter"><div class="top"><span class="lab">Memory distillation</span><span class="val">64%</span></div><div class="track"><div class="fill" style="width:64%"></div></div></div>
            <div class="meter"><div class="top"><span class="lab">Context budget</span><span class="val">31%</span></div><div class="track thin"><div class="fill f2" style="width:31%"></div></div></div>
          </div>
        </div>

        <!-- data row -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Data Row</h4></div><span class="cap">inspector</span></div>
          <div class="panel-body">
            <div class="drow-group">
              <div class="drow-head"><span class="t">Context</span><span class="rule"></span><span class="mono">4 keys</span></div>
              <div class="drow"><span class="k">Model</span><span class="v">Claude Opus 4.6</span></div>
              <div class="drow"><span class="k">Codex ID</span><span class="v mono">SH-0271</span></div>
              <div class="drow"><span class="k">Status</span><span class="v"><span class="badge b-success" style="font-size:10px"><span class="bdot"></span>Distilled</span></span></div>
              <div class="drow"><span class="k">Budget</span><span class="v metercell"><span class="track"><span class="fill" style="width:31%"></span></span><span class="mono">31%</span></span></div>
            </div>
          </div>
        </div>

        <!-- tree -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Disclosure / Tree</h4></div><span class="cap">campaign</span></div>
          <div class="panel-body">
            <div class="tree">
              <div class="tnode" data-open={treeOpen}>
                <button class="trow" onclick={() => (treeOpen = !treeOpen)}>
                  <svg class="chev" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5 3 9 7 5 11" /></svg>
                  <span class="ti">Objective #O-118 · Refactor Keeper queue</span><span class="tmeta">3 · 12m</span>
                </button>
                <div class="tkids">
                  <div class="tnode"><div class="trow leaf"><span class="chev"></span><span class="ti">Edited keeper/queue.rs</span><span class="tmeta">+42 −11</span></div></div>
                  <div class="tnode"><div class="trow leaf"><span class="chev"></span><span class="ti">Ran cargo test</span><span class="tmeta">ok · 1.4s</span></div></div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- code block -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Code Block</h4></div><span class="cap">tool output</span></div>
          <div class="panel-body">
            <div class="codeblock" data-open={codeOpen}>
              <div class="ch"><span class="lbl">tool_call · run_command</span></div>
              <div class="clip">
                <pre><span class="cm"># cargo test keeper::queue</span>
<span class="ky">$</span> cargo test keeper::queue
<span class="st">ok</span> · 14 passed · 1.42s</pre>
                {#if !codeOpen}<div class="fade"></div>{/if}
              </div>
              <button class="showmore" onclick={() => (codeOpen = !codeOpen)}>{codeOpen ? "show less ▴" : "show more ▾"}</button>
            </div>
          </div>
        </div>

        <!-- event icons (MON-124) -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Event Icons</h4></div><span class="cap">timeline taxonomy</span></div>
          <div class="panel-body">
            <div class="evt-grid" style="grid-template-columns: repeat(3, 1fr)">
              {#each eventKinds as e (e.k)}
                <div class="evt">
                  <span class="ei" class:tone-error={e.k === "blocker"}>
                    <EventIcon kind={e.k} tone={e.k === "blocker" ? "warning" : "neutral"} />
                  </span>
                  <div class="el"><div class="en">{e.n}</div><div class="ek">{e.d}</div></div>
                </div>
              {/each}
            </div>
          </div>
        </div>

        <!-- empty state -->
        <div class="panel">
          <div class="panel-head"><div class="ttl"><h4>Empty State</h4></div><span class="cap">zero data</span></div>
          <div class="empty">
            <div class="glyph" aria-hidden="true"></div>
            <h4>No shadows summoned</h4>
            <p>The bridge is quiet. Extract a shadow to begin your first objective.</p>
            <button class="btn btn-primary" style="margin-top:var(--s2)">Summon shadow</button>
          </div>
        </div>
      </div>
    </section>
  </main>
</div>

<style>
  /* Catalog scaffolding only — ported from the reference sheet (css/sheet.css).
     Component atoms come from the global atoms.css import above. */
  .cat {
    min-height: 100vh;
    background: var(--bg-base);
    color: var(--text-secondary);
    font-family: "Inter", system-ui, sans-serif;
    font-size: 13px;
    line-height: 1.55;
    overflow-y: auto;
    height: 100vh;
  }
  .masthead {
    position: sticky; top: 0; z-index: 20;
    display: flex; align-items: center; justify-content: space-between;
    gap: var(--s5); padding: var(--s4) var(--s6);
    background: var(--bg-sink); border-bottom: 1px solid var(--border);
  }
  .brand { display: flex; align-items: center; gap: var(--s3); }
  .brand .crown { width: 26px; height: 26px; flex: none; border-radius: var(--r-md); background: var(--accent); position: relative; }
  .brand .crown::before { content: ""; position: absolute; inset: 6px; background: var(--accent-ink); clip-path: polygon(0 100%, 0 38%, 25% 60%, 50% 10%, 75% 60%, 100% 38%, 100% 100%); }
  .brand .name { font-size: 15px; font-weight: 700; color: var(--text-primary); letter-spacing: 0.02em; }
  .brand .sub { font-size: 11px; color: var(--text-muted); letter-spacing: 0.14em; text-transform: uppercase; }
  .themebar { display: flex; border: 1px solid var(--border); border-radius: var(--r-md); overflow: hidden; }
  .themebar button { font: inherit; font-size: 11px; font-weight: 500; cursor: pointer; padding: 5px 11px; border: none; background: transparent; color: var(--text-muted); border-right: 1px solid var(--border); transition: background .14s, color .14s; }
  .themebar button:last-child { border-right: none; }
  .themebar button:hover { color: var(--text-secondary); background: var(--bg-raised); }
  .themebar button[aria-pressed="true"] { background: var(--accent); color: var(--accent-ink); font-weight: 600; }

  main { padding: var(--s6) var(--s7) var(--s8); max-width: 1100px; margin: 0 auto; }
  .section { padding-block: var(--s6); border-bottom: 1px solid var(--border-subtle); }
  .section-head { display: flex; align-items: baseline; gap: var(--s4); margin-bottom: var(--s5); }
  .section-head .idx { font-size: 12px; font-weight: 600; color: var(--accent); }
  .section-head h2 { font-size: 23px; font-weight: 600; letter-spacing: -0.01em; color: var(--text-primary); }
  .glabel { display: flex; align-items: center; gap: var(--s3); margin: var(--s5) 0 var(--s4); }
  .glabel .t { font-size: 11px; font-weight: 600; letter-spacing: 0.16em; text-transform: uppercase; color: var(--text-muted); white-space: nowrap; }
  .glabel .rule { height: 1px; background: var(--border-subtle); flex: 1; }
  .note { font-size: 12px; color: var(--text-muted); max-width: 64ch; line-height: 1.65; margin-top: var(--s4); }

  .elev-stack { display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; max-width: 320px; }
  .elev-stack > div { padding: var(--s4) var(--s4) var(--s3); border-bottom: 1px solid var(--border-subtle); }
  .elev-stack > div:last-child { border-bottom: none; }
  .elev-stack .nm { font-size: 11px; color: var(--text-primary); }
  .elev-stack .hx { font-size: 10.5px; color: var(--text-muted); margin-top: 1px; }

  .chiprow { display: flex; flex-wrap: wrap; gap: var(--s3); }
  .csample { display: flex; align-items: center; gap: var(--s3); background: var(--bg-panel); border: 1px solid var(--border-subtle); border-radius: var(--r-md); padding: var(--s2) var(--s3); }
  .csample .sw-dot { width: 30px; height: 30px; border-radius: var(--r-md); border: 1px solid var(--border-subtle); flex: none; }
  .csample .lab { font-size: 11px; color: var(--text-primary); }
  .csample .lab small { display: block; color: var(--text-muted); font-size: 10px; margin-top: 1px; }

  .grade-ramp { display: flex; gap: var(--s2); align-items: flex-end; max-width: 460px; }
  .grade-col { display: flex; flex-direction: column; align-items: center; gap: var(--s2); flex: 1; }
  .grade-badge { width: 100%; aspect-ratio: 1; display: flex; align-items: center; justify-content: center; font-family: "JetBrains Mono", monospace; font-weight: 600; font-size: 20px; border-radius: var(--r-md); border: 1px solid; }
  .grade-col .gx { font-size: 10px; color: var(--text-muted); }

  .text-spec { display: flex; flex-direction: column; border: 1px solid var(--border-subtle); border-radius: var(--r-lg); overflow: hidden; }
  .text-spec .row { display: flex; align-items: center; justify-content: space-between; gap: var(--s4); padding: var(--s4); background: var(--bg-panel); border-bottom: 1px solid var(--border-subtle); }
  .text-spec .row:last-child { border-bottom: none; }
  .text-spec .meta { font-size: 10.5px; color: var(--text-muted); text-align: right; white-space: nowrap; }

  .ruler { display: flex; flex-direction: column; gap: var(--s2); }
  .ruler .r { display: flex; align-items: center; gap: var(--s4); }
  .ruler .r .bar { height: 14px; background: var(--accent); border-radius: var(--r-sm); flex: none; }
  .ruler .r .lab { font-size: 11px; color: var(--text-secondary); }

  .radius-spec { display: flex; gap: var(--s4); flex-wrap: wrap; }
  .radius-spec figure { display: flex; flex-direction: column; gap: var(--s3); align-items: center; }
  .radius-spec .rbox { width: 88px; height: 64px; background: var(--bg-raised); border: 1px solid var(--border-strong); }
  .radius-spec figcaption { font-size: 10.5px; color: var(--text-muted); text-align: center; }
  .radius-spec figcaption b { display: block; color: var(--text-primary); font-size: 11px; }

  .dotlist { display: flex; flex-direction: column; gap: var(--s3); }
  .dotlist .d { display: flex; align-items: center; gap: var(--s3); font-size: 12px; color: var(--text-secondary); }
  .dotlist .d .mono { margin-left: auto; color: var(--text-muted); font-size: 10.5px; }
  .avatars { display: flex; gap: var(--s4); align-items: center; flex-wrap: wrap; margin-top: var(--s2); }
</style>
