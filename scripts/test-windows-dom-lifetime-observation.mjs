import assert from 'node:assert/strict';
import test from 'node:test';
import { chromium } from '@playwright/test';
import { captureListenerTargets, captureSubscriptionScopes, installDomLifetimeObservation } from './windows-dom-lifetime-observation.mjs';

test('weak DOM probe distinguishes controls and listener removal without retaining CDP handles', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage(), session = await page.context().newCDPSession(page);
    await page.setContent('<main id="root">' + [0,1,2,3].map(i => `<section data-pane="${i}"><div class="view-toolbar"></div><div role="listbox"><div aria-posinset="1">synthetic</div></div></section>`).join('') + '</main>');
    await page.evaluate(() => {
      for (const list of document.querySelectorAll('[role=listbox]')) {
        const listeners = new Set([() => {}]);
        const listing = { displayCache: { pages: new Map([[0, [{id:'synthetic'}]]]) }, state: {total:1,status:'complete'}, actions:{tasks:[]}, windows:[] };
        const store = { listing, subscribe: listener => { listeners.add(listener); return () => listeners.delete(listener); } };
        list.__reactFiber$fixture = { memoizedState: {memoizedState:store,next:null} };
      }
      const list = document.querySelector('[role=listbox]');
      window.__testScroll = () => {};
      list.addEventListener('scroll', window.__testScroll);
    });
    await page.evaluate(installDomLifetimeObservation);
    await page.evaluate(() => window.__projectpaneLifetimeProbe.capture('before'));
    const metadata = await page.evaluate(() => window.__projectpaneLifetimeProbe.snapshot());
    const id = metadata.records.find(r => r.labels.includes('before:pane-0:list')).id;
    assert.equal(metadata.stores[0].cachedEntries, 1);
    const expression = `window.__projectpaneLifetimeProbe.get(${id})`;
    const before = await captureListenerTargets(session, [{label:'list',expression}]);
    assert(before[0].listeners.some(l => l.type === 'scroll'));
    const scopes = await captureSubscriptionScopes(session, 0);
    assert(scopes.some(s => s.collections.some(c => c.description === 'Set(1)')), JSON.stringify(scopes));
    await page.evaluate(() => { const list=document.querySelector('[role=listbox]');list.removeEventListener('scroll',window.__testScroll);delete window.__testScroll;list.remove(); });
    const after = await captureListenerTargets(session, [{label:'removed-list',expression}]);
    assert.equal(after[0].count, 0);
    const positive = await captureListenerTargets(session, [{label:'control',expression:'window.__projectpaneLifetimeProbe.retainedControl'}]);
    assert.equal(positive[0].listeners[0].type, 'diagnostic-control');
    // The same reader also releases all acquired handles when a later query fails.
    let released = 0, groupReleased = false;
    const fake = {send: async (method) => {
      if(method==='Runtime.evaluate')return {result:{objectId:'target'}};
      if(method==='DOMDebugger.getEventListeners')throw Error('unsupported');
      if(method==='Runtime.releaseObject')released++;
      if(method==='Runtime.releaseObjectGroup')groupReleased=true;
      return {};
    }};
    await assert.rejects(captureListenerTargets(fake,[{label:'failure',expression:'window'}]), /unsupported/);
    assert.equal(released,1);assert.equal(groupReleased,true);
    await session.detach();
  } finally { await browser.close(); }
});
