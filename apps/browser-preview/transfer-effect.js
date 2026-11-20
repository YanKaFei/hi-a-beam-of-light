(function(root){
'use strict';
const DURATION=3600;
const clamp=x=>Math.max(0,Math.min(1,x)),smooth=x=>{x=clamp(x);return x*x*(3-2*x)};
function draw(c,w,h,ms,reduced=false){
 c.clearRect(0,0,w,h);const y=h*.38,left=w*.24,right=w*.76,size=Math.min(76,w*.16),rise=Math.min(100,w*.2);
 const t=smooth((ms-400)/1800),awake=smooth(ms/350),arrived=smooth((ms-1900)/500);
 function glow(x,y,r,color,a){const g=c.createRadialGradient(x,y,0,x,y,r);g.addColorStop(0,`rgba(${color},${a})`);g.addColorStop(.22,`rgba(${color},${a*.48})`);g.addColorStop(1,`rgba(${color},0)`);c.fillStyle=g;c.beginPath();c.arc(x,y,r,0,Math.PI*2);c.fill()}
 function phone(x,receiving){c.strokeStyle='rgba(230,220,208,.4)';c.lineWidth=1.2;const ph=size*1.8,pw=size;const r=12;c.beginPath();c.moveTo(x-pw/2+r,y-ph/2);c.lineTo(x+pw/2-r,y-ph/2);c.quadraticCurveTo(x+pw/2,y-ph/2,x+pw/2,y-ph/2+r);c.lineTo(x+pw/2,y+ph/2-r);c.quadraticCurveTo(x+pw/2,y+ph/2,x+pw/2-r,y+ph/2);c.lineTo(x-pw/2+r,y+ph/2);c.quadraticCurveTo(x-pw/2,y+ph/2,x-pw/2,y+ph/2-r);c.lineTo(x-pw/2,y-ph/2+r);c.quadraticCurveTo(x-pw/2,y-ph/2,x-pw/2+r,y-ph/2);c.stroke();c.beginPath();c.moveTo(x-9,y-ph/2+9);c.lineTo(x+9,y-ph/2+9);c.stroke();glow(x,y,size*.85,'246,168,95',(receiving?arrived*.8:.68)*awake)}
 phone(left,false);phone(right,true);
 if(!reduced){
  for(let k=0;k<14;k++){
   c.beginPath();const start=Math.max(0,t-.42),end=t;
   for(let i=0;i<=65;i++){const u=start+(end-start)*i/65,x=left+(right-left)*u,py=y-Math.sin(Math.PI*u)*(rise+k*.9)+Math.sin(u*18+k*.8)*k*.35;
    if(i===0)c.moveTo(x,py);else c.lineTo(x,py)}
   c.strokeStyle=k<4?`rgba(255,238,192,${.35*awake})`:`rgba(${k%2?'240,124,89':'165,138,224'},${.09*awake})`;c.lineWidth=k<4?1.2:3;c.stroke();
  }
  for(let i=0;i<42;i++){const u=clamp(t-(i%12)*.012),x=left+(right-left)*u,py=y-Math.sin(Math.PI*u)*rise+Math.sin(i*2.1+ms/450)*(i%5)*1.8;glow(x,py,2+(i%3),'255,208,136',.45*awake)}
  glow(left+(right-left)*t,y-Math.sin(Math.PI*t)*rise,38,'255,214,158',.9*awake);
 }
 if(arrived){for(let i=0;i<3;i++){const p=clamp((ms-1900-i*170)/1000);c.beginPath();c.arc(right,y,8+p*size*1.4,0,Math.PI*2);c.strokeStyle=`rgba(246,182,115,${(1-p)*.6})`;c.lineWidth=1;c.stroke()}glow(right,y,60,'255,183,126',arrived*.55)}
}
const api={DURATION,draw};if(typeof module==='object'&&module.exports)module.exports=api;else root.FirstLightTransferEffect=api;
})(typeof window!=='undefined'?window:globalThis);
