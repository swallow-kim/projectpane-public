import assert from 'node:assert/strict';
import test from 'node:test';
import { chromium } from '@playwright/test';
import { observeReconnectListeners } from './windows-reconnect-listeners.mjs';

test('listener observer labels tool registrations and releases handles without changing app listeners', async () => {
  const browser = await chromium.launch({headless:true});
  try {
    const page = await browser.newPage();
    await page.setContent('<button>synthetic</button>');
    await page.evaluate(() => {
      window.__fixtureCalls=0;
      window.addEventListener('fixture-app',()=>window.__fixtureCalls++);
      class InjectedScript {
        constructor() {
          this.window=window;this.document=document;
          const owner=this;
          const listener=event=>owner._hitTargetInterceptor?.call(owner,event);
          window.addEventListener('fixture-tool',listener);
        }
      }
      new InjectedScript();
    });
    const session=await page.context().newCDPSession(page);
    const first=await observeReconnectListeners(session,'first');
    assert.equal(first.toolCount,1);
    assert(first.listeners.some(l=>l.type==='fixture-app'&&!l.tool));
    const tool=first.listeners.find(l=>l.type==='fixture-tool');
    assert(tool.scopeOwners.some(s=>s.objects.some(o=>o.className==='InjectedScript'&&o.ref)));
    assert(first.realms.some(r=>r.rows.some(row=>row.constructor==='InjectedScript'&&row.ownsWindow&&row.ownsDocument)));
    // A fresh CDP session must still see the same identity, not a diagnostic-owned strong copy.
    await session.detach();
    const next=await page.context().newCDPSession(page);
    const second=await observeReconnectListeners(next,'second');
    assert.deepEqual(second.listeners.find(l=>l.type==='fixture-tool').ref,tool.ref);
    await page.evaluate(()=>window.dispatchEvent(new Event('fixture-app')));
    assert.equal(await page.evaluate(()=>window.__fixtureCalls),1);
    await next.detach();
  } finally { await browser.close(); }
});

test('failed listener query releases handles and removes the protocol observer',async()=>{
  let removed=false, released=0,groupReleased=false;
  const session={on(){},off(){removed=true;},send:async method=>{
    if(method==='Runtime.evaluate')return {result:{objectId:'window'}};
    if(method==='DOMDebugger.getEventListeners')throw Error('query failure');
    if(method==='Runtime.releaseObject')released++;
    if(method==='Runtime.releaseObjectGroup')groupReleased=true;
    return {};
  }};
  await assert.rejects(observeReconnectListeners(session,'failure'),/query failure/);
  assert(removed&&groupReleased);assert.equal(released,1);
});
