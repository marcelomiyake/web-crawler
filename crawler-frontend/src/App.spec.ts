import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App.vue'

const job = {
  id: 'job-1', status: 'completed', max_urls: 500, max_depth: 3,
  created_at: '2026-09-24T10:00:00Z', completed_at: '2026-09-24T10:00:05Z',
  counts: { queued: 0, leased: 0, completed: 1, skipped: 0, failed: 0 },
}
const page = {
  id: 'page-1', canonical_url: 'https://example.com/', title: 'Example', http_status: 200, depth: 0,
  fetched_at: '2026-09-24T10:00:04Z',
}

describe('archive workspace', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.restoreAllMocks()
    vi.stubGlobal('confirm', vi.fn(() => true))
  })

  it('shows server validation and service errors without hiding the form', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: 'seed_host_not_allowed' }), { status: 422 })))
    const wrapper = mount(App)
    await wrapper.find('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain('seed host not allowed')
    expect(wrapper.find('form').exists()).toBe(true)
    wrapper.unmount()
  })

  it('requires a seed URL and an exact host before submitting', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    const wrapper = mount(App)

    await wrapper.get('#seeds').setValue('')
    await wrapper.find('form').trigger('submit')

    expect(wrapper.get('[role="alert"]').text()).toContain('Add at least one seed URL')
    expect(fetchMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('shows crawl creation feedback through the semantic output element', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/crawl-jobs') && init?.method === 'POST') {
        return new Response(JSON.stringify({ id: 'job-created', status: 'running', created_at: job.created_at }), { status: 202 })
      }
      if (url.endsWith('/pages')) return new Response(JSON.stringify({ pages: [], next_cursor: null }))
      return new Response(JSON.stringify({ ...job, id: 'job-created', status: 'running' }))
    }))
    const wrapper = mount(App)

    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.get('output.message-success').text()).toContain('Crawl job created.')
    expect(wrapper.find('[role="status"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('renders archived markup as text and does not create active elements', async () => {
    localStorage.setItem('web-crawler.recent-jobs.v1', JSON.stringify(['job-1']))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/content')) return new Response('<script>bad()</script><img src=x onerror=bad()>')
      if (url.endsWith('/pages')) return new Response(JSON.stringify({ pages: [page], next_cursor: null }))
      return new Response(JSON.stringify(job))
    }))
    const wrapper = mount(App)
    await flushPromises()
    await wrapper.get('.page-row').trigger('click')
    await flushPromises()
    expect(wrapper.get('.snapshot-content').text()).toContain('<script>bad()</script>')
    expect(wrapper.find('script').exists()).toBe(false)
    expect(wrapper.find('img').exists()).toBe(false)
    wrapper.unmount()
  })

  it('switches jobs, loads another page, and closes the saved snapshot', async () => {
    localStorage.setItem('web-crawler.recent-jobs.v1', JSON.stringify(['job-1', 'job-2']))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/content')) return new Response('<p>snapshot</p>')
      if (url.includes('/pages')) {
        return new Response(JSON.stringify({ pages: url.includes('cursor=') ? [] : [page], next_cursor: url.includes('cursor=') ? null : 'next-page' }))
      }
      const id = url.endsWith('job-2') ? 'job-2' : 'job-1'
      return new Response(JSON.stringify({ ...job, id }))
    }))
    const wrapper = mount(App)
    await flushPromises()

    await wrapper.findAll('.job-item')[1]!.trigger('click')
    await flushPromises()
    await wrapper.get('.load-more').trigger('click')
    await flushPromises()
    await wrapper.get('.page-row').trigger('click')
    await flushPromises()

    expect(wrapper.get('.snapshot-content').text()).toContain('<p>snapshot</p>')
    await wrapper.get('.snapshot-header button').trigger('click')
    expect(wrapper.find('.snapshot').exists()).toBe(false)
    wrapper.unmount()
  })

  it('deletes a crawl and removes its recent job identifier', async () => {
    localStorage.setItem('web-crawler.recent-jobs.v1', JSON.stringify(['job-1']))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      if (init?.method === 'DELETE') return new Response(null, { status: 204 })
      if (String(input).endsWith('/pages')) return new Response(JSON.stringify({ pages: [], next_cursor: null }))
      return new Response(JSON.stringify(job))
    }))
    const wrapper = mount(App)
    await flushPromises()

    await wrapper.get('button.button-quiet').trigger('click')
    await flushPromises()

    expect(wrapper.get('output.message-success').text()).toContain('Crawl and saved page content deleted.')
    expect(localStorage.getItem('web-crawler.recent-jobs.v1')).toBe('[]')
    expect(wrapper.find('.job-item').exists()).toBe(false)
    wrapper.unmount()
  })
})
