// Test-runner cleanup for Playwright 1.63's injected input checker. Never imported by product code.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';

const hitTypes=new Set(['mousemove','pointerdown','pointerup','touchstart','touchend','touchcancel','mousedown','mouseup','click','auxclick','dblclick','contextmenu']);
const checkType='__playwright_global_listeners_check__';
const normalize=source=>(source??'').replace(/\s+/g,' ').trim();
const hitSource=normalize('(event) => { var _a; return (_a = this._hitTargetInterceptor) == null ? void 0 : _a.call(this, event); }');
const checkSource='() => seenEvent = true';
const signature=l=>JSON.stringify([l.contextId,l.type,l.useCapture,l.scriptId,l.lineNumber,l.columnNumber]);

async function withListeners(session,callback) {
  const group=`playwright-listener-cleanup-${Date.now()}`,handles=new Set();
  const contexts=new Map(),created=({context})=>contexts.set(context.id,context);
  session.on?.('Runtime.executionContextCreated',created);
  const query=async()=>{
    const listeners=[];
    assert(contexts.size<=16,'bounded context inventory');
    for(const contextId of (contexts.size?[...contexts.keys()]:[undefined])){
      const w=await session.send('Runtime.evaluate',{expression:'window',objectGroup:group,...(contextId===undefined?{}:{contextId})});
      assert(w.result.objectId,'window reference available');handles.add(w.result.objectId);
      const result=await session.send('DOMDebugger.getEventListeners',{objectId:w.result.objectId,depth:1});
      for(const l of result.listeners){
        for(const h of [l.handler,l.originalHandler])if(h?.objectId)handles.add(h.objectId);
        listeners.push({...l,contextId,contextName:contexts.get(contextId)?.name,isDefault:contexts.get(contextId)?.auxData?.isDefault});
      }
    }
    return listeners;
  };
  // This module uses its own diagnostic CDP session. Re-enabling inventories all worlds,
  // including Chromium's utility world and WebView2's main-world injection.
  try{await session.send('Runtime.disable');await session.send('Runtime.enable');return await callback(await query(),query);}
  finally{
    const released=await Promise.allSettled([...handles].map(objectId=>session.send('Runtime.releaseObject',{objectId})));
    await session.send('Runtime.releaseObjectGroup',{objectGroup:group});
    session.off?.('Runtime.executionContextCreated',created);
    assert(released.every(r=>r.status==='fulfilled'),'listener cleanup handles released');
  }
}

export async function captureListenerBaseline(session) {
  const token=`__projectpaneInputBaseline_${randomUUID().replaceAll('-','')}`;
  await withListeners(session,async listeners=>{
    for(const l of listeners){
      const handler=l.originalHandler??l.handler;assert(handler?.objectId,'baseline listener identity available');
      const result=await session.send('Runtime.callFunctionOn',{objectId:handler.objectId,
        functionDeclaration:'function(token){(globalThis[token]??=new WeakSet()).add(this);}',arguments:[{value:token}],returnByValue:true});
      assert(!result.exceptionDetails,'weak connection baseline registered');
    }
  });
  return token;
}

export async function removeOwnedInputListeners(session,baselineToken) {
  assert(/^__projectpaneInputBaseline_[a-f0-9]{32}$/.test(baselineToken),'connection baseline required');
  return withListeners(session,async(before,query)=>{
    const candidates=[];
    for(const l of before){
      const source=normalize((l.originalHandler??l.handler)?.description);
      if(!((hitTypes.has(l.type)&&l.useCapture&&source===hitSource)||
        (l.type===checkType&&!l.useCapture&&source===checkSource)))continue;
      const existing=await session.send('Runtime.callFunctionOn',{objectId:(l.originalHandler??l.handler).objectId,
        functionDeclaration:'function(token){return globalThis[token]?.has(this)??false;}',arguments:[{value:baselineToken}],returnByValue:true});
      assert(!existing.exceptionDetails,'connection baseline read');
      if(!existing.result.value)candidates.push(l);
    }
    const scripts=[...new Set(candidates.map(l=>l.scriptId))];
    // Validate every complete registration group before making any changes.
    if(scripts.length)await session.send('Debugger.enable');
    try{
      for(const scriptId of scripts){
        const group=candidates.filter(l=>l.scriptId===scriptId);
        assert.equal(group.length,13,'complete Playwright input-listener group');
        assert.equal(new Set(group.map(l=>l.type)).size,13,'unique Playwright input-listener events');
        const {scriptSource}=await session.send('Debugger.getScriptSource',{scriptId});
        assert(['_setupGlobalListenersRemovalDetection','_setupHitTargetInterceptors','onGlobalListenersRemoved','_allHitTargetInterceptorEvents',checkType].every(text=>scriptSource.includes(text)),
          'verified Playwright injected script');
      }
    }finally{if(scripts.length)await session.send('Debugger.disable');}
    for(const l of candidates){
      const handler=l.originalHandler??l.handler;assert(handler?.objectId,'owned listener handle');
      const result=await session.send('Runtime.callFunctionOn',{objectId:handler.objectId,
        functionDeclaration:'function(type,capture){window.removeEventListener(type,this,capture);}',
        arguments:[{value:l.type},{value:l.useCapture}],returnByValue:true});
      assert(!result.exceptionDetails,'owned listener removal completed');
    }
    const remaining=await query(),removed=new Set(candidates);
    assert.deepEqual(remaining.map(signature).sort(),before.filter(l=>!removed.has(l)).map(signature).sort(),'only connection-owned listeners removed');
    return {removed:candidates.length,scriptGroups:scripts.length,remaining:remaining.length,
      groups:scripts.map(scriptId=>{const rows=candidates.filter(l=>l.scriptId===scriptId);return {scriptId,count:rows.length,contextName:rows[0].contextName,isDefault:rows[0].isDefault};}),
      limitation:'Removes window input-checker registrations only; does not dispose Playwright internal MutationObserver or claim owner/heap collection.'};
  });
}

export async function releaseListenerBaseline(session,token){
  assert(/^__projectpaneInputBaseline_[a-f0-9]{32}$/.test(token),'connection baseline required');
  await withListeners(session,async listeners=>{
    for(const l of listeners){
      const handler=l.originalHandler??l.handler;assert(handler?.objectId,'baseline release identity');
      const r=await session.send('Runtime.callFunctionOn',{objectId:handler.objectId,
        functionDeclaration:'function(token){delete globalThis[token];}',arguments:[{value:token}],returnByValue:true});
      assert(!r.exceptionDetails,'connection baseline released');
    }
    const r=await session.send('Runtime.evaluate',{expression:`delete globalThis[${JSON.stringify(token)}]`,returnByValue:true});
    assert(!r.exceptionDetails,'main realm baseline released');
  });
}
