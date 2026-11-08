<script lang="ts">
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import AuthGuard from "$lib/components/AuthGuard.svelte";
  import { Card, Spinner, EmptyState, ErrorCard, Field, Input, Textarea, Button, Badge } from "$lib/ui";

  let folders = $state<string[]>([]);
  let selectedFolder = $state("INBOX");
  let messages = $state<any[]>([]);
  let selectedUid = $state<number | null>(null);
  let full = $state<any>(null);
  let loadingFolders = $state(true);
  let loadingMessages = $state(false);
  let err = $state("");
  let info = $state("");
  let searchQuery = $state("");
  let searchResults = $state<any[]>([]);
  let searching = $state(false);

  let meEmail = $state("");
  let quotaUsed = $state(0);
  let quotaTotal = $state(0);

  let composeMode = $state(false);
  let composeTo = $state("");
  let composeSubject = $state("");
  let composeBody = $state("");
  let sendPending = $state(false);

  async function loadFolders() {
    loadingFolders = true;
    err = "";
    try {
      const r: any = await Api.mailFolders();
      folders = r.folders ?? r.Folders ?? [];
      if (folders.length && !folders.includes(selectedFolder)) selectedFolder = folders[0];
      await loadMessages(selectedFolder);
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      loadingFolders = false;
    }
  }

  async function loadMessages(folder: string) {
    loadingMessages = true;
    err = "";
    full = null;
    selectedUid = null;
    try {
      const r: any = await Api.mailMessages(folder, 50);
      messages = r.messages ?? [];
      if (messages.length) {
        selectedUid = messages[0].uid;
        await loadFull();
      }
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      loadingMessages = false;
    }
  }

  async function loadFull() {
    if (selectedUid == null) { full = null; return; }
    try {
      full = await Api.mailMessage(selectedUid);
    } catch (e: any) {
      err = e?.message ?? String(e);
    }
  }

  async function doSearch() {
    const q = searchQuery.trim();
    if (!q) {
      searchResults = [];
      await loadMessages(selectedFolder);
      return;
    }
    searching = true;
    err = "";
    try {
      const r: any = await Api.mailSearch(q);
      searchResults = r.results ?? [];
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      searching = false;
    }
  }

  async function sendMail() {
    if (sendPending) return;
    if (!composeTo.trim() || !composeSubject.trim()) {
      err = "To and subject are required.";
      return;
    }
    sendPending = true;
    err = "";
    info = "";
    try {
      await Api.mailSend({ from: meEmail, to: composeTo.trim(), subject: composeSubject.trim(), body: composeBody });
      info = "Message sent.";
      composeTo = "";
      composeSubject = "";
      composeBody = "";
      composeMode = false;
      await loadMessages(selectedFolder);
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      sendPending = false;
    }
  }

  function selectMessage(uid: any) {
    selectedUid = Number(uid);
    loadFull();
  }

  $effect(() => {
    if (!Api.getToken()) { goto("/login"); return; }
    (async () => {
      try {
        const m: any = await Api.me();
        meEmail = m.email ?? "";
      } catch {}
      try {
        const q: any = await Api.quota();
        quotaUsed = Number(q.used_mb ?? 0);
        quotaTotal = Number(q.quota_mb ?? 0);
      } catch {}
      await loadFolders();
    })();
  });
</script>

<AuthGuard>
  {#if loadingFolders}
    <div class="p-6"><Spinner /></div>
  {:else}
    <div class="flex flex-col md:flex-row min-h-[70vh] md:h-[calc(100vh-7rem)] w-full overflow-hidden rounded-3xl border border-divider bg-surface/60 backdrop-blur-xl shadow-2xl shadow-black/30">
      <!-- Column 1: navigation -->
      <aside class="w-full md:w-[18%] bg-surface/80 border-b md:border-b-0 md:border-r border-divider flex flex-col p-4 gap-4 overflow-y-auto">
        <div class="flex items-center gap-2 px-1">
          <span class="grid place-items-center w-9 h-9 rounded-xl bg-gradient-to-br from-brand to-info shadow-lg shadow-brand/20">
            <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4h16v16H4z M4 8l8 5 8-5"/></svg>
          </span>
          <span class="font-bold text-lg tracking-tight">Mail</span>
        </div>
        <button
          class="bg-gradient-to-r from-brand to-info hover:opacity-90 text-text-primary font-medium px-4 py-2.5 rounded-xl shadow-lg shadow-brand/30 active:scale-[0.98] transition-all"
          onclick={() => (composeMode = true)}
        >
          + Compose
        </button>
        <nav class="flex flex-col gap-1">
          {#each folders as f}
            <button
              type="button"
              onclick={() => { composeMode = false; selectedFolder = f; loadMessages(f); }}
              class="flex items-center gap-2 text-left w-full px-3 py-2 rounded-xl transition-all {selectedFolder === f && !composeMode ? 'bg-brand/15 text-text-primary shadow-inner' : 'text-text-muted hover:text-text-primary hover:bg-elevated/70'}"
            >
              <span class="w-2 h-2 rounded-full {selectedFolder === f && !composeMode ? 'bg-brand' : 'bg-text-dim/40'}"></span>
              {f}
            </button>
          {/each}
        </nav>
        <div class="mt-auto pt-3 border-t border-divider">
          <div class="text-xs text-text-muted mb-1">Storage</div>
          <div class="h-2 bg-elevated rounded-full overflow-hidden">
            <div class="h-2 bg-brand rounded-full" style="width: {quotaTotal ? Math.min(100, quotaUsed / quotaTotal * 100) : 0}%"></div>
          </div>
          <div class="text-xs text-text-dim mt-1">{quotaUsed} / {quotaTotal} MB</div>
        </div>
      </aside>

      <!-- Column 2: mailbox list -->
      <section class="w-full md:w-[32%] bg-appBg border-b md:border-b-0 md:border-r border-divider flex flex-col overflow-y-auto p-3 max-h-[50vh] md:max-h-none">
        <div class="flex gap-2 mb-3">
          <input
            bind:value={searchQuery}
            placeholder="Search…"
            class="flex-1 bg-appBg border border-divider text-text-primary placeholder:text-text-dim rounded-xl px-4 py-3 base-transition focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing"
            onkeydown={(e) => { if (e.key === "Enter") doSearch(); }}
          />
          <button onclick={doSearch} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-3 py-2 rounded-xl transition-colors">
            {searching ? "…" : "Go"}
          </button>
        </div>
        {#if err}<ErrorCard error={err} />{/if}
        {#if info}<p class="text-success text-sm mb-2">{info}</p>{/if}

        {#if searchQuery.trim() && searchResults.length}
          <div class="text-xs text-text-muted mb-2">Search results</div>
          {#each searchResults as r}
            <div class="bg-surface border border-divider rounded-xl p-3 mb-2">
              <div class="font-semibold text-sm">{r.subject ?? ""}</div>
              <div class="text-xs text-text-muted">{r.folder ?? ""}</div>
            </div>
          {/each}
        {:else if loadingMessages}
          <Spinner />
        {:else if messages.length === 0}
          <EmptyState text="No messages." />
        {:else}
          <div class="flex flex-col gap-2">
            {#each messages as m (m.uid)}
              <button
                type="button"
                onclick={() => selectMessage(m.uid)}
                class="group text-left w-full rounded-2xl border transition-all p-3 {selectedUid === m.uid && !composeMode ? 'bg-brand/15 border-brand/40 shadow-lg shadow-brand/10' : 'bg-surface/70 border-divider hover:bg-elevated/70'}"
              >
                <div class="flex items-start gap-3">
                  <div class="grid place-items-center w-10 h-10 rounded-xl bg-gradient-to-br from-brand to-info text-sm font-bold shrink-0 shadow shadow-brand/20">
                    {(m.from ?? "?").toString().trim().slice(0, 1).toUpperCase()}
                  </div>
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center justify-between gap-2">
                      <span class="text-sm font-semibold truncate {m.seen ? 'text-text-muted' : 'text-text-primary'}">{m.from ?? ""}</span>
                      <span class="text-[11px] text-text-dim whitespace-nowrap">{m.date ?? ""}</span>
                    </div>
                    <div class="text-sm font-medium truncate mt-0.5 {m.seen ? '' : 'text-text-primary'}">{m.subject ?? "(no subject)"}</div>
                    <div class="text-xs text-text-muted truncate mt-0.5">{m.preview ?? ""}</div>
                  </div>
                  {#if !m.seen}
                    <span class="w-2 h-2 rounded-full bg-brand mt-1.5 shrink-0"></span>
                  {/if}
                </div>
              </button>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Column 3: reading pane -->
      <main class="flex-1 bg-surface flex flex-col p-6 overflow-y-auto">
        {#if composeMode}
          <div class="max-w-3xl">
            <h2 class="text-2xl font-bold mb-4 bg-gradient-to-r from-brand to-info bg-clip-text text-transparent">Compose message</h2>
            <div class="rounded-2xl border border-divider bg-appBg/40 p-5 space-y-3">
              <Field label="From"><Input bind:value={meEmail} /></Field>
              <Field label="To"><Input bind:value={composeTo} placeholder="recipient@example.com" /></Field>
              <Field label="Subject"><Input bind:value={composeSubject} placeholder="Subject" /></Field>
              <Field label="Body"><Textarea bind:value={composeBody} rows={12} /></Field>
              <div class="flex gap-3 pt-2">
                <Button variant="primary" onclick={sendMail} disabled={sendPending}>{sendPending ? "Sending…" : "Send"}</Button>
                <Button variant="secondary" onclick={() => (composeMode = false)}>Cancel</Button>
              </div>
            </div>
          </div>
        {:else if full}
          <div class="max-w-4xl">
            <h2 class="text-3xl font-bold break-words leading-tight">{full.subject ?? "(no subject)"}</h2>
            <div class="mt-4 flex items-center gap-3">
              <div class="grid place-items-center w-11 h-11 rounded-xl bg-gradient-to-br from-brand to-info text-sm font-bold shadow shadow-brand/20">
                {(full.from ?? "?").toString().trim().slice(0, 1).toUpperCase()}
              </div>
              <div class="min-w-0 flex-1">
                <div class="font-semibold">{full.from ?? ""}</div>
                <div class="text-xs text-text-muted">{full.date ?? ""}</div>
              </div>
              {#if !full.seen}
                <span class="bg-info/10 text-info text-xs font-semibold px-2.5 py-1 rounded-full border border-info/20">unread</span>
              {/if}
            </div>
            <div class="mt-6 rounded-2xl border border-divider bg-appBg/40 p-6 whitespace-pre-wrap break-words text-text-primary leading-relaxed">
              {full.raw ?? ""}
            </div>
          </div>
        {:else}
          <div class="grid place-items-center h-full"><EmptyState text="Select a message to read." /></div>
        {/if}
      </main>
    </div>
  {/if}
</AuthGuard>
