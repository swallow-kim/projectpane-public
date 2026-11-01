import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp,rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:net';
import { chromium } from '@playwright/test';
import { captureListenerBaseline,removeOwnedInputListeners,releaseListenerBaseline } from './windows-playwright-listener-cleanup.mjs';

test('CDP reconnect cleanup removes only current input-checker registrations and preserves prior/app listeners',async()=>{
  const server=createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const port=server.address().port;await new Promise(resolve=>server.close(resolve));
  const profile=await mkdtemp(join(tmpdir(),'projectpane-listener-regression-'));
  let context,connection;
  try{
    context=await chromium.launchPersistentContext(profile,{headless:true,args:[`--remote-debugging-port=${port}`,'--remote-debugging-address=127.0.0.1']});
    const original=context.pages()[0];await original.setContent('<button id="action">synthetic</button>');
    await original.evaluate(()=>{
      window.__appEvents=0;
      window.addEventListener('click',()=>window.__appEvents++);
      // Same type is insufficient to identify ownership.
      window.addEventListener('__playwright_global_listeners_check__',()=>window.__appEvents+=10);
    });
    await original.locator('#action').click(); // Pre-existing tool registrations must be preserved too.
    let expectedCount;
    for(let cycle=0;cycle<3;cycle++){
      connection=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      const page=connection.contexts()[0].pages()[0],session=await page.context().newCDPSession(page);
      const baseline=await captureListenerBaseline(session);
      await page.locator('#action').click();
      const result=await removeOwnedInputListeners(session,baseline);
      assert.equal(result.removed,13);assert.equal(result.scriptGroups,1);
      expectedCount??=result.remaining;assert.equal(result.remaining,expectedCount);
      assert.equal((await removeOwnedInputListeners(session,baseline)).removed,0,'cleanup is idempotent');
      await releaseListenerBaseline(session,baseline);
      await session.detach();await connection.close();connection=undefined;
    }
    assert.equal(await original.evaluate(()=>window.__appEvents),4,'all actual clicks delivered to app');
    await original.evaluate(()=>window.dispatchEvent(new CustomEvent('__playwright_global_listeners_check__')));
    assert.equal(await original.evaluate(()=>window.__appEvents),14,'app same-name listener preserved');
    await original.locator('#action').click();
    assert.equal(await original.evaluate(()=>window.__appEvents),15,'pre-existing input owner still works');
  }finally{
    await connection?.close();await context?.close();
    // mkdtemp returns the exact owned test profile; no user profile or computed parent is removed.
    assert(profile.startsWith(join(tmpdir(),'projectpane-listener-regression-')));
    await rm(profile,{recursive:true,force:true});
  }
});

test('incomplete or unrelated listener group fails closed before removal',async()=>{
  let removed=0,released=0;
  const listener={type:'click',useCapture:true,scriptId:'owned',handler:{objectId:'handler',description:'(event) => { var _a; return (_a = this._hitTargetInterceptor) == null ? void 0 : _a.call(this, event); }'}};
  const session={send:async (method,args)=>{
    if(method==='Runtime.evaluate')return {result:{objectId:'window'}};
    if(method==='DOMDebugger.getEventListeners')return {listeners:[listener]};
    if(method==='Runtime.callFunctionOn'){
      if(args.functionDeclaration.includes('removeEventListener'))removed++;
      return {result:{value:false}};
    }
    if(method==='Runtime.releaseObject')released++;
    return {};
  }};
  await assert.rejects(removeOwnedInputListeners(session,'__projectpaneInputBaseline_'+'0'.repeat(32)),/complete Playwright/);
  assert.equal(removed,0);assert.equal(released,2);
});
