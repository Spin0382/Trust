<script lang="ts">
  import { onMount } from "svelte";
  import { MAX_MOTD_CHARS, fetchConfig, setMotd, type ConfigSnapshot } from "./api";

  let { session }: { session: string } = $props();

  let config = $state<ConfigSnapshot | null>(null);
  let error = $state("");
  let motdDraft = $state("");
  let saving = $state(false);
  let saved = $state(false);
  // Counted as Unicode scalar values, matching the backend's `chars().count()` rather than
  // JavaScript's UTF-16 `.length`.
  let motdLength = $derived([...motdDraft].length);

  async function refresh() {
    try {
      config = await fetchConfig(session);
      motdDraft = config.motd;
      error = "";
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  onMount(refresh);

  // The same rules the backend enforces, checked here so an overlong or multi-line draft is not
  // sent at all. The server stays authoritative: a client-side check is a convenience, not a gate.
  function validateMotd(value: string): string {
    const length = [...value].length;
    if (length > MAX_MOTD_CHARS) {
      return `the message of the day is limited to ${MAX_MOTD_CHARS} characters; this one is ${length}`;
    }
    if (/[\u0000-\u001f\u007f]/.test(value) || /[\u2028\u2029]/.test(value)) {
      return "the message of the day must be a single line without control characters";
    }
    return "";
  }

  async function saveMotd() {
    const problem = validateMotd(motdDraft);
    if (problem) {
      saved = false;
      error = problem;
      return;
    }
    saving = true;
    saved = false;
    try {
      await setMotd(session, motdDraft);
      if (config) config.motd = motdDraft;
      saved = true;
      // A save that follows a failed one must not leave the old error sitting next to the new
      // "saved."  -  without this, a failure followed by a success showed both at once.
      error = "";
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  // "saved." otherwise never clears: it stayed shown under a draft the operator had since edited
  // further, which reads as "your new changes are saved" when they are not.
  function onMotdInput() {
    saved = false;
  }
</script>

<h2>settings</h2>

{#if error}<p class="danger">{error}</p>{/if}

{#if config}
  <section>
    <h3 class="dim">message of the day</h3>
    <p class="dim">
      this is the one setting the panel can change live: the game task reads it fresh every time a
      player joins, so a save here applies to the very next join. everything in the grid below is
      read-only, with the reason beside it. those values are fixed when the process starts or are
      only read from the config file, so a new value takes a restart.
    </p>
    <form class="motd-form" onsubmit={(e) => { e.preventDefault(); saveMotd(); }}>
      <input
        bind:value={motdDraft}
        oninput={onMotdInput}
        maxlength={MAX_MOTD_CHARS}
        aria-label="message of the day"
      />
      <span class="count">{motdLength}/{MAX_MOTD_CHARS}</span>
      <button disabled={saving}>{saving ? "saving…" : "save"}</button>
    </form>
    {#if saved}<p class="accent-text">saved.</p>{/if}
  </section>

  <section class="grid">
    <div class="card">
      <span class="label">listen address</span>
      <span class="value">{config.listen}</span>
      <span class="hint">read-only: bound when the server starts. change listen and restart.</span>
    </div>
    <div class="card">
      <span class="label">max players</span>
      <span class="value">{config.max_players}</span>
      <span class="hint">
        read-only: the player slot table is sized at startup. change max_players and restart.
      </span>
    </div>
    <div class="card">
      <span class="label">world size</span>
      <span class="value">{config.world_width} × {config.world_height}</span>
      <span class="hint">read-only: the loaded world's dimensions. switch worlds to change them.</span>
    </div>
    <div class="card">
      <span class="label">password</span>
      <span class="value">{config.password_set ? "set" : "none (open server)"}</span>
      <span class="hint">
        read-only: set or clear password in the config and restart. the value is never shown here.
      </span>
    </div>
    <div class="card">
      <span class="label">max chat length</span>
      <span class="value">{config.max_chat_len}</span>
      <span class="hint">read-only: change max_chat_len and restart.</span>
    </div>
    <div class="card">
      <span class="label">idle timeout</span>
      <span class="value">{config.idle_timeout_secs}s</span>
      <span class="hint">read-only: change idle_timeout_secs and restart.</span>
    </div>
    <div class="card">
      <span class="label">autosave</span>
      <span class="value">
        {config.autosave_secs > 0 ? `every ${config.autosave_secs}s` : "disabled"}
      </span>
      <span class="hint">read-only: change autosave_secs and restart.</span>
    </div>
    <div class="card">
      <span class="label">save destination</span>
      <span class="value small">{config.save_target ?? "none (this world will not be saved)"}</span>
      <span class="hint">read-only: the world file is opened at startup. use the worlds tab to switch.</span>
    </div>
    <div class="card">
      <span class="label">whitelist</span>
      <span class="value">
        {config.whitelist_on ? `on (${config.whitelist_count})` : "off"}
      </span>
      <span class="hint">managed live on the whitelist tab.</span>
    </div>
  </section>
{:else}
  <p class="dim">loading…</p>
{/if}

<style>
  h2 {
    margin: 0 0 1rem;
    font-size: 1rem;
    font-weight: 600;
  }

  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.8rem;
    letter-spacing: 0.06em;
    font-weight: 500;
  }

  section {
    margin-bottom: 1.75rem;
  }

  section > p {
    max-width: 46em;
  }

  .accent-text {
    color: var(--accent);
  }

  .motd-form {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    max-width: 640px;
  }

  .motd-form input {
    flex: 1;
  }

  .count {
    font-size: 0.75rem;
    color: var(--text-dim);
    white-space: nowrap;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 0.75rem;
    max-width: 800px;
  }

  .card {
    border: 1px solid var(--border);
    background: var(--bg-raised);
    border-radius: var(--radius-lg);
    padding: 0.9rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .label {
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }

  .value {
    font-size: 1.05rem;
    color: var(--accent);
    word-break: break-word;
  }

  .value.small {
    font-size: 0.82rem;
  }

  .hint {
    font-size: 0.76rem;
    line-height: 1.35;
    color: var(--text-dim);
  }
</style>
