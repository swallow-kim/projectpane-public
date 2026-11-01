import assert from 'node:assert/strict';

function registerWeak(label) {
  const key = '__projectpaneReconnectRefs';
  if (!globalThis[key]) {
    const rows = [], finalized = [], ids = new WeakMap();
    const registry = new FinalizationRegistry(id => finalized.push({id,at:performance.now()}));
    const track = (label,target) => {
      let id = ids.get(target);
      if(id === undefined) {
        if(rows.length>=64)throw Error('reconnect_ref_limit');
        id=rows.length;ids.set(target,id);rows.push({id,labels:[],weak:new WeakRef(target)});registry.register(target,id);
      }
      if(!rows[id].labels.includes(label))rows[id].labels.push(label);
      return {realm:globalThis[key].realm,id};
    };
    globalThis[key] = {realm:`${performance.timeOrigin}-${Math.random()}`,track,snapshot:()=>({realm:globalThis[key].realm,finalized:[...finalized],rows:rows.map(r=>{
      const value=r.weak.deref();return {id:r.id,labels:[...r.labels],alive:value!==undefined,type:typeof value,
        constructor:value?.constructor?.name, ownsWindow:value?.window===window,ownsDocument:value?.document===document,
        interceptorActive:value?._hitTargetInterceptor!==undefined};
    })})};
    track('unretained-control',()=>{});
  }
  return globalThis[key].track(label,this);
}

export async function observeReconnectListeners(session, stage) {
  const contexts = new Map();
  const created = ({context}) => contexts.set(context.id,context);
  session.on('Runtime.executionContextCreated',created);
  const group = `reconnect-${Date.now()}`, handles = new Set();
  const props = async objectId => {
    const r=await session.send('Runtime.getProperties',{objectId,ownProperties:true});
    for(const p of [...r.result,...(r.internalProperties??[])])if(p.value?.objectId)handles.add(p.value.objectId);
    return r;
  };
  const track = async (objectId,label) => {
    const r=await session.send('Runtime.callFunctionOn',{objectId,functionDeclaration:registerWeak.toString(),arguments:[{value:label}],returnByValue:true});
    assert(!r.exceptionDetails,`weak reference registration: ${r.exceptionDetails?.exception?.description??''}`);return r.result.value;
  };
  try {
    await session.send('Runtime.enable');
    const w=await session.send('Runtime.evaluate',{expression:'window',objectGroup:group});handles.add(w.result.objectId);
    const raw=await session.send('DOMDebugger.getEventListeners',{objectId:w.result.objectId,depth:1});
    assert(raw.listeners.length<=256);
    const listeners=[],seen=new Set();
    for(const l of raw.listeners){
      for(const h of [l.handler,l.originalHandler])if(h?.objectId)handles.add(h.objectId);
      const handler=l.originalHandler??l.handler;
      const tool=l.type==='__playwright_global_listeners_check__'||handler?.description?.includes('_hitTargetInterceptor');
      const row={type:l.type,capture:l.useCapture,scriptId:l.scriptId,line:l.lineNumber,column:l.columnNumber,tool:Boolean(tool),target:'window'};
      if(tool){
        assert(handler?.objectId,'tool handler reference available');
        row.ref=await track(handler.objectId,`${stage}:${l.type}`);
        const id=`${row.ref.realm}:${row.ref.id}`;
        if(!seen.has(id)){
          seen.add(id);row.scopeOwners=[];
          const internals=await props(handler.objectId),scopes=internals.internalProperties?.find(p=>p.name==='[[Scopes]]')?.value.objectId;
          if(scopes)for(const scope of (await props(scopes)).result.filter(p=>/^\d+$/.test(p.name)).slice(0,4)){
            if(!scope.value?.objectId||!/^(Closure|Block|Local)/.test(scope.value.description??''))continue;
            const members=(await props(scope.value.objectId)).result;
            row.scopeOwners.push({kind:scope.value.description,objects:members.filter(p=>p.value?.className==='InjectedScript'||p.value?.description==='InjectedScript').map(p=>({name:p.name,className:p.value.className}))});
            for(const p of members.filter(p=>p.value?.className==='InjectedScript'||p.value?.description==='InjectedScript'))
              row.scopeOwners.at(-1).objects.find(x=>x.name===p.name).ref=await track(p.value.objectId,`${stage}:InjectedScript-owner`);
          }
        }
      }
      listeners.push(row);
    }
    const realms=[];
    for(const context of contexts.values()){
      const r=await session.send('Runtime.evaluate',{contextId:context.id,expression:'globalThis.__projectpaneReconnectRefs?.snapshot()??null',returnByValue:true});
      assert(!r.exceptionDetails,'realm observation unavailable');
      if(r.result.value)realms.push({contextName:context.name,isDefault:context.auxData?.isDefault,...r.result.value});
    }
    return {stage,utc:new Date().toISOString(),listeners,toolCount:listeners.filter(l=>l.tool).length,appCount:listeners.filter(l=>!l.tool).length,realms};
  } finally {
    const released=await Promise.allSettled([...handles].map(objectId=>session.send('Runtime.releaseObject',{objectId})));
    await session.send('Runtime.releaseObjectGroup',{objectGroup:group});
    session.off('Runtime.executionContextCreated',created);
    assert(released.every(r=>r.status==='fulfilled'),'reconnect diagnostic handles released');
  }
}
