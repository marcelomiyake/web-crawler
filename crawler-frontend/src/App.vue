<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'

type FrontierCounts = { queued: number; leased: number; completed: number; skipped: number; failed: number }
type Job = {
  id: string
  status: 'running' | 'completed' | 'completed_with_errors'
  max_urls: number
  max_depth: number
  created_at: string
  completed_at: string | null
  counts: FrontierCounts
}
type Page = { id: string; canonical_url: string; title: string; http_status: number; depth: number; fetched_at: string }
type PageList = { pages: Page[]; next_cursor: string | null }

const RECENT_KEY = 'web-crawler.recent-jobs.v1'
const seedsInput = ref('https://example.com/')
const hostsInput = ref('example.com')
const jobs = ref<Job[]>([])
const selectedJobId = ref('')
const pages = ref<Page[]>([])
const selectedPage = ref<Page | null>(null)
const pageContent = ref('')
const nextCursor = ref<string | null>(null)
const loading = ref(false)
const submitting = ref(false)
const errorMessage = ref('')
const noticeMessage = ref('')
let refreshTimer: number | undefined

const selectedJob = computed(() => jobs.value.find((job) => job.id === selectedJobId.value) ?? null)
const progressTotal = computed(() => selectedJob.value
  ? selectedJob.value.counts.queued + selectedJob.value.counts.leased + selectedJob.value.counts.completed + selectedJob.value.counts.skipped + selectedJob.value.counts.failed
  : 0)
const progressPercent = computed(() => progressTotal.value === 0
  ? 0
  : Math.min(100, Math.round(((selectedJob.value?.counts.completed ?? 0) + (selectedJob.value?.counts.skipped ?? 0) + (selectedJob.value?.counts.failed ?? 0)) / progressTotal.value * 100)))

function lines(value: string): string[] {
  return value.split(/\r?\n/).map((line) => line.trim()).filter(Boolean)
}

function recentIds(): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]')
    return Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === 'string').slice(0, 5) : []
  } catch {
    return []
  }
}

function remember(id: string): void {
  const ids = [id, ...recentIds().filter((existing) => existing !== id)].slice(0, 5)
  localStorage.setItem(RECENT_KEY, JSON.stringify(ids))
}

async function responseError(response: Response): Promise<string> {
  try {
    const body = await response.json() as { error?: string }
    return body.error ? `Request failed: ${body.error.replaceAll('_', ' ')}.` : `Request failed (${response.status}).`
  } catch {
    return `Request failed (${response.status}).`
  }
}

async function loadJob(id: string): Promise<void> {
  const response = await fetch(`/api/v1/crawl-jobs/${encodeURIComponent(id)}`)
  if (!response.ok) {
    if (response.status === 404) {
      localStorage.setItem(RECENT_KEY, JSON.stringify(recentIds().filter((existing) => existing !== id)))
      return
    }
    throw new Error(await responseError(response))
  }
  const job = await response.json() as Job
  const index = jobs.value.findIndex((item) => item.id === job.id)
  if (index === -1) jobs.value.unshift(job)
  else jobs.value[index] = job
}

async function loadPages(cursor?: string): Promise<void> {
  if (!selectedJobId.value) return
  loading.value = true
  try {
    const query = cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''
    const response = await fetch(`/api/v1/crawl-jobs/${encodeURIComponent(selectedJobId.value)}/pages${query}`)
    if (!response.ok) throw new Error(await responseError(response))
    const result = await response.json() as PageList
    pages.value = cursor ? [...pages.value, ...result.pages] : result.pages
    nextCursor.value = result.next_cursor
    if (selectedPage.value) {
      const updated = pages.value.find((page) => page.id === selectedPage.value?.id)
      if (updated) selectedPage.value = updated
    }
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Could not load saved pages.'
  } finally {
    loading.value = false
  }
}

async function refreshSelected(): Promise<void> {
  const id = selectedJobId.value
  if (!id) return
  try {
    await loadJob(id)
    await loadPages()
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Could not refresh this crawl.'
  }
}

async function createJob(): Promise<void> {
  errorMessage.value = ''
  noticeMessage.value = ''
  const seeds = lines(seedsInput.value)
  const allowedHosts = lines(hostsInput.value)
  if (seeds.length === 0 || allowedHosts.length === 0) {
    errorMessage.value = 'Add at least one seed URL and one exact hostname.'
    return
  }
  submitting.value = true
  try {
    const response = await fetch('/api/v1/crawl-jobs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Idempotency-Key': crypto.randomUUID() },
      body: JSON.stringify({ seeds, allowed_hosts: allowedHosts }),
    })
    if (!response.ok) throw new Error(await responseError(response))
    const created = await response.json() as { id: string }
    remember(created.id)
    selectedJobId.value = created.id
    selectedPage.value = null
    pageContent.value = ''
    pages.value = []
    await refreshSelected()
    noticeMessage.value = 'Crawl job created. The worker will observe robots.txt and the host delay before fetching pages.'
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Could not create the crawl job.'
  } finally {
    submitting.value = false
  }
}

async function selectJob(id: string): Promise<void> {
  errorMessage.value = ''
  noticeMessage.value = ''
  selectedJobId.value = id
  selectedPage.value = null
  pageContent.value = ''
  pages.value = []
  await refreshSelected()
}

async function openPage(page: Page): Promise<void> {
  if (!selectedJobId.value) return
  errorMessage.value = ''
  selectedPage.value = page
  pageContent.value = ''
  try {
    const response = await fetch(`/api/v1/crawl-jobs/${encodeURIComponent(selectedJobId.value)}/pages/${encodeURIComponent(page.id)}/content`)
    if (!response.ok) throw new Error(await responseError(response))
    pageContent.value = await response.text()
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Could not load this page snapshot.'
  }
}

async function deleteJob(): Promise<void> {
  if (!selectedJobId.value || !window.confirm('Delete this crawl and all of its saved page content? This cannot be undone.')) return
  errorMessage.value = ''
  try {
    const response = await fetch(`/api/v1/crawl-jobs/${encodeURIComponent(selectedJobId.value)}`, { method: 'DELETE' })
    if (!response.ok) throw new Error(await responseError(response))
    const deletedId = selectedJobId.value
    jobs.value = jobs.value.filter((job) => job.id !== deletedId)
    localStorage.setItem(RECENT_KEY, JSON.stringify(recentIds().filter((id) => id !== deletedId)))
    selectedJobId.value = ''
    pages.value = []
    selectedPage.value = null
    pageContent.value = ''
    noticeMessage.value = 'Crawl and saved page content deleted.'
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Could not delete this crawl.'
  }
}

onMounted(async () => {
  const ids = recentIds()
  await Promise.all(ids.map((id) => loadJob(id).catch(() => undefined)))
  if (jobs.value.length > 0) {
    selectedJobId.value = jobs.value[0]!.id
    await loadPages()
  }
  refreshTimer = window.setInterval(() => {
    void refreshSelected()
  }, 3_000)
})

onUnmounted(() => {
  if (refreshTimer !== undefined) window.clearInterval(refreshTimer)
})
</script>

<template>
  <div class="app-shell">
    <header class="topbar">
      <a
        class="brand"
        href="#top"
        aria-label="Fieldnote home"
      >
        <span
          class="brand-mark"
          aria-hidden="true"
        ><span /><span /><span /></span>
        <span class="brand-name">fieldnote<span class="brand-dot">.</span></span>
      </a>
      <div class="topbar-meta">
        <span class="local-dot" /><span>LOCAL ARCHIVE</span><span class="meta-divider" /><span>WEB CRAWLER</span>
      </div>
      <a
        class="topbar-link"
        href="https://marcelomiyake.com.br/posts/context-engineering/"
        target="_blank"
        rel="noreferrer"
      >How it works <span aria-hidden="true">↗</span></a>
    </header>

    <main
      id="top"
      class="page-layout"
    >
      <section
        class="intro-block"
        aria-labelledby="page-title"
      >
        <p class="eyebrow">
          <span class="eyebrow-line" /> A SMALL, CAREFUL CRAWLER
        </p>
        <h1 id="page-title">
          Keep a fieldnote<br>of the <em>open web.</em>
        </h1>
        <p class="intro-copy">
          Collect a bounded set of HTML pages from hosts you name. Every crawl follows site rules, moves slowly, and stays on this machine.
        </p>
      </section>

      <div
        v-if="errorMessage"
        class="message message-error"
        role="alert"
      >
        <span aria-hidden="true">!</span>{{ errorMessage }}
      </div>
      <output
        v-if="noticeMessage"
        class="message message-success"
      >
        <span aria-hidden="true">✓</span>{{ noticeMessage }}
      </output>

      <div class="workspace-grid">
        <aside
          class="sidebar"
          aria-label="Your archive"
        >
          <div class="sidebar-heading">
            <span>YOUR ARCHIVE</span><span class="count-pill">{{ jobs.length }}</span>
          </div>
          <div
            v-if="jobs.length === 0"
            class="archive-empty"
          >
            <div
              class="empty-symbol"
              aria-hidden="true"
            >
              ⌁
            </div>
            <p>No crawls yet.</p>
            <span>Your saved jobs will gather here.</span>
          </div>
          <nav
            v-else
            class="job-list"
            aria-label="Recent crawl jobs"
          >
            <button
              v-for="job in jobs"
              :key="job.id"
              class="job-item"
              :class="{ selected: job.id === selectedJobId }"
              @click="selectJob(job.id)"
            >
              <span
                class="job-item-icon"
                aria-hidden="true"
              >{{ job.status === 'running' ? '◷' : job.status === 'completed' ? '✓' : '!' }}</span>
              <span class="job-item-copy"><strong>{{ new Date(job.created_at).toLocaleDateString() }}</strong><small>{{ job.counts.completed }} saved · {{ job.counts.failed + job.counts.skipped }} skipped</small></span>
              <span
                v-if="job.status === 'running'"
                class="running-pip"
                aria-label="Running"
              />
            </button>
          </nav>
          <div class="sidebar-note">
            <span
              class="lock-icon"
              aria-hidden="true"
            >⌑</span><p>Private by design.<br><span>No public endpoint. No accounts.</span></p>
          </div>
        </aside>

        <section
          class="main-column"
          aria-label="Crawl workspace"
        >
          <section
            class="panel create-panel"
            aria-labelledby="create-title"
          >
            <div class="panel-header">
              <div>
                <p class="section-index">
                  01 / NEW COLLECTION
                </p><h2 id="create-title">
                  Start a crawl
                </h2>
              </div><span class="bounded-label"><span class="bounded-dot" /> BOUNDED</span>
            </div>
            <form
              class="crawl-form"
              @submit.prevent="createJob"
            >
              <label for="seeds">Seed URLs <span class="label-note">one per line · up to 20</span></label>
              <textarea
                id="seeds"
                v-model="seedsInput"
                rows="3"
                spellcheck="false"
                autocomplete="url"
                placeholder="https://docs.example.org/"
                required
              />
              <label for="hosts">Exact allowed hostnames <span class="label-note">one per line · subdomains are separate</span></label>
              <textarea
                id="hosts"
                v-model="hostsInput"
                rows="2"
                spellcheck="false"
                autocomplete="off"
                placeholder="docs.example.org"
                required
              />
              <div class="form-footer">
                <p>
                  <span
                    class="info-icon"
                    aria-hidden="true"
                  >i</span> Up to 500 pages · 3 links deep · 2 MiB each
                </p><button
                  class="button button-primary"
                  type="submit"
                  :disabled="submitting"
                >
                  <span>{{ submitting ? 'Starting…' : 'Start collection' }}</span><span
                    class="button-arrow"
                    aria-hidden="true"
                  >↗</span>
                </button>
              </div>
            </form>
          </section>

          <section
            v-if="selectedJob"
            class="panel results-panel"
            aria-labelledby="results-title"
          >
            <div class="panel-header results-header">
              <div>
                <p class="section-index">
                  02 / COLLECTION
                </p><h2 id="results-title">
                  Crawl progress
                </h2>
              </div><div class="header-actions">
                <span
                  class="status-pill"
                  :class="`status-${selectedJob.status}`"
                ><span />{{ selectedJob.status.replaceAll('_', ' ') }}</span><button
                  class="button button-quiet"
                  type="button"
                  @click="deleteJob"
                >
                  Delete
                </button>
              </div>
            </div>
            <div class="job-progress">
              <div class="progress-heading">
                <span>{{ selectedJob.counts.completed }} <span>pages saved</span></span><span>{{ progressPercent }}%</span>
              </div>
              <progress
                class="progress-track"
                :value="progressPercent"
                max="100"
                aria-label="Crawl completion"
              />
              <div class="progress-stats">
                <span><i class="stat-queued" />{{ selectedJob.counts.queued }} queued</span><span><i class="stat-active" />{{ selectedJob.counts.leased }} in progress</span><span><i class="stat-done" />{{ selectedJob.counts.skipped + selectedJob.counts.failed }} skipped / failed</span>
              </div>
            </div>
            <div class="pages-heading">
              <div><h3>Saved pages</h3><span>{{ pages.length }} shown</span></div><span
                v-if="loading"
                class="subtle-loading"
              >Refreshing…</span>
            </div>
            <div
              v-if="pages.length === 0"
              class="pages-empty"
            >
              <span aria-hidden="true">⌁</span><p>{{ selectedJob.status === 'running' ? 'The frontier is ready.' : 'No HTML pages were saved.' }}</p><small>{{ selectedJob.status === 'running' ? 'The worker observes robots.txt and host pacing before each request.' : 'Skipped and failed URLs remain visible in the crawl counts.' }}</small>
            </div>
            <ul
              v-else
              class="page-list"
            >
              <li
                v-for="page in pages"
                :key="page.id"
              >
                <button
                  class="page-row"
                  :class="{ 'page-selected': selectedPage?.id === page.id }"
                  @click="openPage(page)"
                >
                  <span class="page-number">{{ page.depth === 0 ? 'SEED' : `0${page.depth}` }}</span><span class="page-copy"><strong>{{ page.title || page.canonical_url }}</strong><small>{{ page.canonical_url }}</small></span><span class="http-code">{{ page.http_status }}</span><span
                    class="row-arrow"
                    aria-hidden="true"
                  >↗</span>
                </button>
              </li>
            </ul>
            <button
              v-if="nextCursor"
              class="load-more"
              type="button"
              :disabled="loading"
              @click="loadPages(nextCursor)"
            >
              Load more pages <span aria-hidden="true">↓</span>
            </button>
            <section
              v-if="selectedPage"
              class="snapshot"
              aria-labelledby="snapshot-title"
            >
              <div class="snapshot-header">
                <div>
                  <p class="section-index">
                    PAGE SNAPSHOT
                  </p><h3 id="snapshot-title">
                    {{ selectedPage.title || 'Untitled page' }}
                  </h3>
                </div><button
                  class="button button-quiet"
                  type="button"
                  @click="selectedPage = null; pageContent = ''"
                >
                  Close
                </button>
              </div>
              <p class="snapshot-url">
                {{ selectedPage.canonical_url }}
              </p>
              <pre
                class="snapshot-content"
                aria-label="Escaped HTML source"
              >{{ pageContent }}</pre>
            </section>
          </section>

          <section
            v-else
            class="panel first-run-panel"
            aria-label="Archive guidance"
          >
            <span
              class="first-run-mark"
              aria-hidden="true"
            >✳</span><div>
              <p class="section-index">
                A NOTE BEFORE YOU BEGIN
              </p><h2>Start with a small, trusted site.</h2><p>Each hostname is matched exactly. The crawler blocks private network addresses, checks robots.txt, limits retries, and stores snapshots until you delete the job.</p>
            </div>
          </section>

          <div class="safety-strip">
            <span
              class="safety-icon"
              aria-hidden="true"
            >◈</span><p><strong>Respectful by default</strong><span>One request per host per second · robots.txt honored · no JavaScript execution</span></p><a
              href="https://www.rfc-editor.org/rfc/rfc9309.html"
              target="_blank"
              rel="noreferrer"
              aria-label="Read RFC 9309"
            >↗</a>
          </div>
        </section>
      </div>

      <footer class="page-footer">
        <span>FIELDNOTE / WEB CRAWLER</span><span>LOCAL TOOL · HTML ONLY · MANUAL RETENTION</span><span>v0.1.0</span>
      </footer>
    </main>
  </div>
</template>
