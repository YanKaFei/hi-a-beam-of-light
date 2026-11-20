const {test}=require('node:test');
const assert=require('node:assert/strict');
const opening=require('../opening.js');
test('subtitle cues allow reading holds and never overlap',()=>{
 let end=0;
 for(const cue of opening.CUES){assert.ok(cue.from>=end);assert.ok(cue.to-cue.from>=2000);assert.equal(opening.cueAt(cue.from),cue);assert.ok(opening.opacityAt((cue.from+cue.to)/2,cue)>.9);end=cue.to}
 assert.ok(end<opening.DURATION);
 assert.equal(opening.cueAt(opening.DURATION),null);
});
test('every phase renders finite coordinates on narrow and wide canvases',()=>{
 const numeric=new Set(['clearRect','translate','rotate','scale','arc','moveTo','quadraticCurveTo','lineTo']);
 const ctx=new Proxy({}, {get:(_,key)=>key==='createRadialGradient'? (...args)=>{assert.ok(args.every(Number.isFinite));return {addColorStop:(at)=>assert.ok(at>=0&&at<=1)}}:(...args)=>{if(numeric.has(key))assert.ok(args.every(Number.isFinite),key)},set:()=>true});
 for(const [w,h] of [[320,568],[390,844],[1200,800]])
 for(const reduced of [false,true])
 for(let ms=0;ms<=opening.DURATION;ms+=100)opening.draw(ctx,w,h,ms,{reduced,target:{x:w/2,y:200}});
});
