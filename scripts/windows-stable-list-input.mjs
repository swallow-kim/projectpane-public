import assert from 'node:assert/strict';
import { expect } from '@playwright/test';

// A watcher may invalidate a ready listing between two CDP commands. Retry only
// an End event proven to have reached that listing while aria-busy was true.
// Missing/wrong-target input and ready-list failures remain failures.
export async function pressEndWhenReady(page,pane,lastPosition,{detailedTrace=true}={}){
 const list=pane.getByRole('listbox'),attempts=[];
 try{
  for(let attempt=1;attempt<=3;attempt++){
   await expect(list).toHaveAttribute('aria-busy','false');
   await list.evaluate((element,detailed)=>{
    const events=[],trace=[];let dropped=0;
    const state=kind=>{
     const rows=[...element.querySelectorAll('[aria-posinset]')];
     return {kind,at:performance.now(),busy:element.getAttribute('aria-busy'),top:element.scrollTop,height:element.clientHeight,extent:element.scrollHeight,
      first:rows[0]?.getAttribute('aria-posinset'),last:rows.at(-1)?.getAttribute('aria-posinset'),
      focused:element.querySelector('[data-focused="true"]')?.getAttribute('aria-posinset'),
      selected:rows.filter(row=>row.getAttribute('aria-selected')==='true').slice(0,32).map(row=>row.getAttribute('aria-posinset'))};
    };
    const recordState=kind=>{if(!detailed)return;if(trace.length<96)trace.push(state(kind));else dropped++;};
    const sequence=()=>Reflect.get(window,'__PROJECTPANE_IPC_OBSERVATION__')?.snapshot().lastSeq;
    const record=event=>{if(event.key==='End'&&events.length<2){events.push({key:event.key,code:event.code,keyCode:event.keyCode,isComposing:event.isComposing,focused:document.hasFocus(),correctTarget:event.target===element,busyAtInput:element.getAttribute('aria-busy'),preventedBefore:event.defaultPrevented,ipcSequenceBefore:sequence()});recordState('key-capture');}};
    const after=event=>{if(event.key==='End'&&events.length){Object.assign(events.at(-1),{preventedAfter:event.defaultPrevented,ipcSequenceAfter:sequence()});recordState('key-bubble');}};
    const focus=event=>{if(event.target===window)recordState(event.type);};
    const scroll=()=>recordState('scroll');
    const observer=detailed?new MutationObserver(()=>recordState('listing-change')):null;
    observer?.observe(element,{attributes:true,subtree:true,childList:true,attributeFilter:['aria-busy','aria-activedescendant','aria-selected']});
    window.addEventListener('keydown',record,true);window.addEventListener('keydown',after);
    if(detailed){window.addEventListener('focus',focus);window.addEventListener('blur',focus);element.addEventListener('scroll',scroll);}
    Reflect.set(window,'__stableEndProbe',{events,finish:()=>{observer?.disconnect();window.removeEventListener('keydown',record,true);window.removeEventListener('keydown',after);if(detailed){window.removeEventListener('focus',focus);window.removeEventListener('blur',focus);element.removeEventListener('scroll',scroll);}return {events,trace,dropped,last:detailed?state('finished'):null,ipcSequenceLast:sequence()};}});
   },detailedTrace);
   const evidence={attempt,events:[]};attempts.push(evidence);let passed=false,primaryError;
   try{
   await list.press('End');
   const events=await page.evaluate(()=>Reflect.get(window,'__stableEndProbe')?.events??[]);
   evidence.events=events;
   assert.equal(events.length,1,'one End event must reach the page');
   assert.equal(events[0].correctTarget,true,'End must target the requested list');
   assert.equal(events[0].isComposing,false,'IME composition is not a ready-list input');
   assert.notEqual(events[0].keyCode,229,'IME key events are outside this probe');
   if(events[0].busyAtInput==='true')continue;
   assert.equal(events[0].busyAtInput,'false','listing readiness must be explicit');
   await expect(pane.locator(`[aria-posinset="${lastPosition}"]`)).toBeVisible();
   passed=true;
   return attempts;
   }catch(error){primaryError=error;throw error;}
   finally{
    let timer;
    try{
     const final=await Promise.race([page.evaluate(()=>{const probe=Reflect.get(window,'__stableEndProbe');Reflect.deleteProperty(window,'__stableEndProbe');return probe?.finish();}),new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('end_probe_cleanup_timeout')),2000);})]);
     if(final){evidence.events=final.events;if(!passed)evidence.observation=final;}
    }catch(error){evidence.observationError=String(error);if(!primaryError)throw error;}
    finally{clearTimeout(timer);}
   }
  }
  throw new Error('listing became busy during all three bounded End attempts');
 }catch(error){error.endInputAttempts=attempts;throw error;}
}
