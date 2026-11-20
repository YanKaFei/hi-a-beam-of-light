/* 一束光 / Brand opening. Absolute-time drawing, repeatable and seekable.
 * Reading holds separate the transfer, multiplication and final welcome.
 * No light here represents actual usage or an activation event.
 */
(function(root){
'use strict';
const ring=typeof module==='object'&&module.exports?require('./ignition-ring.js'):root.FirstLightRing;
const DURATION=17600;
const CUES=[
 {from:1100,to:4600,text:'每个人，都有第一次。',kind:'opening'},
 {from:5750,to:9450,text:'一句「我陪你」，\n就能点亮新的可能。',kind:'companion'},
 {from:10800,to:13750,text:'光不因分享而减少。',kind:'sharing'},
 {from:14600,to:16900,text:'hi，一束光',kind:'welcome'}
];
const clamp=x=>Math.max(0,Math.min(1,x));
const ease=x=>{x=clamp(x);return x*x*(3-2*x)};
const mix=(a,b,t)=>a+(b-a)*ease(t);
function cueAt(ms){return CUES.find(c=>ms>=c.from&&ms<c.to)||null}
function opacityAt(ms,cue){return Math.min(ease((ms-cue.from)/650),ease((cue.to-ms)/550))}
function draw(ctx,w,h,ms,{reduced=false,target=null}={}){
 ctx.clearRect(0,0,w,h);
 const cx=w/2,cy=h*.37,scale=Math.min(w,600)/420;
 const gap=Math.min(w*.22,100),handoff=ease((ms-5000)/650),settle=ease((ms-14000)/700);
 const finalX=target?target.x:cx,finalY=target?target.y:cy;
 const centerX=mix(cx,finalX,settle),centerY=mix(cy,finalY,settle);
 // Preserve the original multi-filament ring as the opening's main image.
 // Warm gold grows into coral/red, moving highlights orbit, then the whole ring contracts.
 if(ms<4800){
  const timeline=[[0,0],[800,400],[2500,1150],[3600,1600],[4350,2050],[4800,2399]];
  let ringTime=2399;
  for(let i=0;i<timeline.length-1;i++){
   const [a,va]=timeline[i],[b,vb]=timeline[i+1];
   if(ms<=b){ringTime=va+(vb-va)*clamp((ms-a)/(b-a));break}
  }
  const shrink=1-.94*ease((ms-3600)/1200);
  const size=Math.min(w*.86,470),factor=size/400*shrink;
  ctx.save();ctx.translate(cx,cy);ctx.scale(factor,factor);ctx.translate(-200,-160);
  ring.draw(ctx,400,400,ringTime,{reduce:reduced,flowPhase:reduced?0:ms/1050});
  ctx.restore();
 }
 // Delay the existing transfer so the ring has time to breathe and resolve.
 ms=Math.max(0,ms-1400);
 const wake=ease(ms/900);
 function seed(x,y,r,alpha=1,warm=false){
  if(alpha<=0)return;
  const g=ctx.createRadialGradient(x,y,0,x,y,r*8);
  const color=warm?'190,115,78':'208,157,88';
  g.addColorStop(0,`rgba(255,243,210,${alpha})`);g.addColorStop(.075,`rgba(${color},${alpha*.96})`);g.addColorStop(.2,`rgba(${color},${alpha*.26})`);g.addColorStop(.5,`rgba(${color},${alpha*.045})`);g.addColorStop(1,`rgba(${color},0)`);
  ctx.fillStyle=g;ctx.beginPath();ctx.arc(x,y,r*8,0,Math.PI*2);ctx.fill();
 }
 const breathe=reduced?1:1+Math.sin(ms/1250)*.035;
 const firstX=mix(cx,cx-gap,handoff),secondX=cx+gap;
 const wholeAlpha=1-ease((ms-12600)/700);
 const coreAlpha=wake*(.65+.35*ease((ms-2700)/700));
 seed(firstX,cy,4.5*scale*breathe,coreAlpha*wholeAlpha);
 // A small light crosses the gap. The original light never dims from giving.
 const travel=clamp((ms-4200)/1350),moving=ms>=4200&&ms<=5650;
 if(moving){const x=mix(cx-gap,secondX,travel),y=cy-Math.sin(travel*Math.PI)*24*scale;
  ctx.save();ctx.strokeStyle=`rgba(210,162,103,${.3*Math.sin(travel*Math.PI)})`;ctx.lineWidth=.8;
  ctx.beginPath();ctx.moveTo(cx-gap,cy);ctx.quadraticCurveTo(cx,cy-48*scale,x,y);ctx.stroke();ctx.restore();seed(x,y,2.4*scale,wholeAlpha);
 }
 const receiver=ease((ms-5450)/850);
 seed(secondX,cy,4.5*scale*breathe,receiver*wholeAlpha,true);
 // Reveal a small field through three waves, holding the field under the brand line.
 const points=[[-.33,-.16],[-.08,-.23],[.25,-.20],[.39,.04],[.18,.20],[-.13,.20],[-.40,.07],[-.24,-.30],[.04,.31],[.40,-.29],[.05,-.39],[-.40,.29]];
 const spread=Math.min(w*.77,470),area=Math.min(h*.39,260);
 for(let i=0;i<points.length;i++){
  const start=8250+Math.floor(i/4)*340,appear=ease((ms-start)/900);
  const x=cx+points[i][0]*spread,y=cy+points[i][1]*area;
  seed(x,y,(2.2+(i%3)*.45)*scale,appear*.72*wholeAlpha,i%3===0);
 }
 // Rest as one light again: this is a brand welcome, never a claimed account state.
 const welcome=ease((ms-12600)/700);
 seed(centerX,centerY,6.3*scale*breathe,welcome*.85);
}
const api={DURATION,CUES,cueAt,opacityAt,draw};
if(typeof module==='object'&&module.exports)module.exports=api;
else root.FirstLightOpening=api;
})(typeof window!=='undefined'?window:(typeof globalThis!=='undefined'?globalThis:{}));
