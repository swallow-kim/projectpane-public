import assert from 'node:assert/strict';
import test from 'node:test';
import { chromium } from '@playwright/test';
import { pressEndWhenReady } from './windows-stable-list-input.mjs';

test('both End trace modes retain ready-input validation, bounded busy retry and cleanup', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const detailedTrace of [true, false]) {
      for (const condition of ['ready', 'busy-once', 'blocked']) {
        await page.setContent('<section><div role="listbox" tabindex="0" aria-busy="false"><div aria-posinset="100000" hidden>last</div></div></section>');
        await page.evaluate(condition => {
          const list = document.querySelector('[role=listbox]');
          let attempts = 0;
          // Capture is installed before the probe: simulate a readiness race at input.
          window.addEventListener('keydown', event => {
            if (event.key !== 'End') return;
            attempts++;
            if (condition === 'busy-once' && attempts === 1) {
              list.setAttribute('aria-busy', 'true');
              setTimeout(() => list.setAttribute('aria-busy', 'false'), 30);
            }
          }, { capture: true });
          list.addEventListener('keydown', event => {
            if (event.key === 'End' && condition !== 'blocked' && list.getAttribute('aria-busy') === 'false') {
              event.preventDefault(); list.firstElementChild.hidden = false;
            }
          });
        }, condition);
        const input = pressEndWhenReady(page, page.locator('section'), 100000, { detailedTrace });
        if (condition === 'blocked') {
          await assert.rejects(input, error => {
            assert.equal(error.endInputAttempts.length, 1, 'ready failure must not retry');
            const observation = error.endInputAttempts[0].observation;
            assert.equal(observation.trace.length > 0, detailedTrace);
            assert.equal(observation.last === null, !detailedTrace);
            return true;
          });
        } else {
          const attempts = await input;
          assert.equal(attempts.length, condition === 'busy-once' ? 2 : 1);
          assert.equal(attempts.at(-1).events[0].busyAtInput, 'false');
          assert.equal(attempts.at(-1).events[0].correctTarget, true);
          assert.equal(await page.locator('[aria-posinset]').isVisible(), true);
        }
        assert.equal(await page.evaluate(() => Reflect.has(window, '__stableEndProbe')), false);
        // New document also removes the fixture's own event listeners.
        await page.goto('about:blank');
      }
    }
  } finally { await browser.close(); }
});
