import test from 'node:test';
import assert from 'node:assert/strict';
import { chromium } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { installInputControl } from './windows-input-focus-observation.mjs';

test('plain input contrast records immediately, preserves host inputs, and cleans up', async () => {
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const events = [];
    await page.exposeFunction('recordFocusProbe', event => events.push(event));
    await page.setContent('<main><section class="pane" aria-label="패널 1"><input aria-label="패널 1 주소" value="unchanged"></section></main>');
    const host = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
    await page.evaluate(installInputControl, { binding: 'recordFocusProbe', lifetimeMs: 600000 });
    const a = page.getByRole('textbox', { name: '일반 입력 A', exact: true });
    const b = page.getByRole('textbox', { name: '일반 입력 B', exact: true });
    await a.click();
    await page.keyboard.insertText('한글');
    await b.click();
    await page.keyboard.press('Backspace');
    assert.equal(await a.inputValue(), '한글');
    assert.equal(await b.inputValue(), '');
    await page.keyboard.insertText('가');
    assert.equal(await b.inputValue(), '가');
    assert.equal(await host.inputValue(), 'unchanged');
    assert.ok(events.some(event => event.type === 'beforeinput' && event.target === 'plain-b'));
    assert.ok(events.some(event => event.type === 'input' && event.target === 'plain-a' && event.trusted));
    mkdirSync('artifacts/p2-native-input-control-20261005/headless', { recursive: true });
    for (const width of [768, 1280]) {
      await page.setViewportSize({ width, height: 900 });
      const panel = await page.locator('#projectpane-input-control').boundingBox();
      assert.ok(panel && panel.x >= 0 && panel.x + panel.width <= width);
      await page.screenshot({ path: `artifacts/p2-native-input-control-20261005/headless/control-${width}.png` });
    }
    await page.getByRole('button', { name: '같은 문제 · 저장 후 종료', exact: true }).click();
    assert.equal(await page.evaluate(() => window.__projectpaneInputControl.snapshot().decision), 'reproduced');
    await page.evaluate(() => window.__projectpaneInputControl.dispose());
    await page.waitForTimeout(50);
    const count = events.length;
    await host.click();
    await page.keyboard.insertText('x');
    assert.equal(events.length, count);
    assert.equal(await page.locator('#projectpane-input-control').count(), 0);
  } finally { await browser.close(); }
});
