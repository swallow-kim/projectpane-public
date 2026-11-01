import assert from 'node:assert/strict';
import test from 'node:test';
import { startSoakObservation, readSoakObservation, stopSoakObservation, failedSoakObservation } from './windows-stable-soak-observation.mjs';

test('uses the bounded observer and captures IPC identity without replacing Tauri invocation', async () => {
  const previousWindow=globalThis.window, previousDocument=globalThis.document;
  let enabled=false;const invoke=()=>{};
  const snapshot=()=>({enabled,capacity:1024,dropped:0,observerErrors:0,events:[{command:'start_listing',phase:'failure',pane:0,generation:2,callId:1}],commands:[]});
  globalThis.window={__TAURI_INTERNALS__:{invoke},__PROJECTPANE_IPC_OBSERVATION__:{enable(){enabled=true;},disable(){enabled=false;},snapshot}};
  globalThis.document={querySelectorAll:()=>[{getAttribute:()=> '0',querySelector:selector=>selector.startsWith('footer')?{textContent:'timeout · 0'}:{getAttribute:()=> 'false'}}]};
  const page={evaluate:async fn=>fn()};
  try {
    await startSoakObservation(page);
    const captured=await readSoakObservation(page);
    assert.equal(captured.ipc.enabled,true);assert.equal(captured.ipc.events[0].command,'start_listing');
    assert.deepEqual(captured.panes,[{pane:'0',status:'timeout · 0',busy:'false'}]);
    const failure=await failedSoakObservation(page,{pane:0,target:'large-fixture'});
    assert.equal(failure.navigation.target,'large-fixture');assert.equal(failure.ipc.events[0].phase,'failure');
    assert.equal(globalThis.window.__TAURI_INTERNALS__.invoke,invoke);
    await stopSoakObservation(page);assert.equal(enabled,false);
  }finally{globalThis.window=previousWindow;globalThis.document=previousDocument;}
});
test('capture failure preserves navigation and cannot replace the original validation error', async()=>{
  const evidence=await failedSoakObservation({evaluate:async()=>{throw Error('page closed');}},{pane:2,target:'small-fixture'});
  assert.deepEqual(evidence,{navigation:{pane:2,target:'small-fixture'},observationError:'Error: page closed'});
});
test('missing diagnostic capability fails explicitly instead of producing a false empty trace', async()=>{
  const previous=globalThis.window;globalThis.window={};
  try{await assert.rejects(startSoakObservation({evaluate:async fn=>fn()}),/soak_ipc_observer_unavailable/);}
  finally{globalThis.window=previous;}
});
test('an unresponsive diagnostic evaluation cannot prevent failure cleanup', async()=>{
  const evidence=await failedSoakObservation({evaluate:()=>new Promise(()=>{})},{pane:1,target:'large-fixture'},20);
  assert.equal(evidence.navigation.pane,1);
  assert.equal(evidence.observationError,'Error: soak_failure_observation_timeout');
});
