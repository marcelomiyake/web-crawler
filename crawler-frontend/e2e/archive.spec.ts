import { expect, test } from '@playwright/test'

const jobId = 'e2e-job-1'
const pageId = 'e2e-page-1'

test('creates a bounded crawl and displays the saved response as text', async ({ page }) => {
  await page.route('**/api/v1/**', async (route) => {
    const request = route.request()
    const url = new URL(request.url())
    if (request.method() === 'POST' && url.pathname.endsWith('/crawl-jobs')) {
      await route.fulfill({ status: 202, contentType: 'application/json', body: JSON.stringify({ id: jobId, status: 'running', created_at: new Date().toISOString() }) })
    } else if (request.method() === 'GET' && url.pathname.endsWith('/content')) {
      await route.fulfill({ status: 200, contentType: 'text/plain; charset=utf-8', body: '<script>not-executed()</script><p>archived source</p>' })
    } else if (request.method() === 'GET' && url.pathname.endsWith('/pages')) {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ pages: [{ id: pageId, canonical_url: 'https://example.com/', title: 'Example', http_status: 200, depth: 0, fetched_at: new Date().toISOString() }], next_cursor: null }) })
    } else if (request.method() === 'GET' && url.pathname.endsWith(`/crawl-jobs/${jobId}`)) {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ id: jobId, status: 'completed', max_urls: 500, max_depth: 3, created_at: new Date().toISOString(), completed_at: new Date().toISOString(), counts: { queued: 0, leased: 0, completed: 1, skipped: 0, failed: 0 } }) })
    } else if (request.method() === 'DELETE' && url.pathname.endsWith(`/crawl-jobs/${jobId}`)) {
      await route.fulfill({ status: 204, body: '' })
    } else {
      await route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'not_found' }) })
    }
  })

  await page.goto('/')
  await page.getByLabel('Seed URLs').fill('https://example.com/')
  await page.getByLabel(/Exact allowed hostnames/).fill('example.com')
  await page.getByRole('button', { name: /Start collection/ }).click()
  await expect(page.getByRole('heading', { name: 'Crawl progress' })).toBeVisible()
  await page.getByRole('button', { name: /Example https:\/\/example.com/ }).click()
  await expect(page.getByLabel('Escaped HTML source')).toContainText('<script>not-executed()</script>')
  await expect(page.locator('.snapshot-content script')).toHaveCount(0)
  page.once('dialog', (dialog) => dialog.accept())
  await page.getByRole('button', { name: 'Delete' }).click()
  await expect(page.getByRole('status')).toContainText('deleted')
  await expect(page.getByRole('heading', { name: 'Crawl progress' })).toHaveCount(0)
})
