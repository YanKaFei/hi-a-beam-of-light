'use strict';
// This preview uses its own local guest sessions; it never claims WeChat identity or AI verification.
let outgoing=null,incoming=new URLSearchParams(location.search).get('light'),pollTimer=null,fxFrame=0,fxPreview=false;
try{outgoing=JSON.parse(sessionStorage.getItem('fl_outgoing')||'null')}catch{}
async function shareApi(path,data){
 const r=await fetch('/api/'+path,{method:data?'POST':'GET',credentials:'same-origin',headers:data?{'content-type':'application/json'}:{},body:data?JSON.stringify(data):undefined,cache:'no-store'});
 let value;try{value=await r.json()}catch{throw Error('请使用新版启动文件运行测试服务，当前服务不支持邀请接口。')}
 if(!r.ok)throw Error(value.error||'请求失败，请重试');return value;
}
const shareStatus=message=>$('transfer-status').textContent=message;
function shareURL(token,base=''){const url=new URL(base||location.origin);url.pathname='/';url.search='';url.hash='';url.searchParams.set('light',token);return url.href}
function showQR(){
 $('invite-panel').hidden=false;$('invite').hidden=true;document.querySelector('.transfer').classList.add('has-invite');
 if(!window.qrcode){shareStatus('二维码组件未加载，请刷新。');return}
 const qr=qrcode(0,'M');qr.addData(outgoing.url);qr.make();$('invite-qr').innerHTML=qr.createSvgTag({cellSize:4,margin:4,scalable:true});
 $('invite-qr').setAttribute('aria-label','传光邀请二维码');
 $('invite-expiry').textContent='邀请有效期 15 分钟。对方扫码打开后，需要主动接收。';
 if(['127.0.0.1','localhost'].includes(new URL(outgoing.url).hostname))shareStatus('当前二维码指向本机，另一台手机无法直接打开。跨设备验收需要可共同访问的 HTTPS 地址；可先预览动效。');
 startPolling();
}
async function createInvite(){
 $('invite').disabled=true;
 try{const r=await shareApi('transfers',{});outgoing={token:r.token,url:shareURL(r.token,r.public_base),expires:r.expires_at,received:false};sessionStorage.setItem('fl_outgoing',JSON.stringify(outgoing));$('invite-label').textContent='等待对方扫码';$('cancel-invite').hidden=false;showQR()}
 catch(e){shareStatus(e.message)}finally{$('invite').disabled=false}
}
function stopPolling(){clearTimeout(pollTimer);pollTimer=null}
function startPolling(){stopPolling();if(document.hidden||!outgoing)return;pollTimer=setTimeout(checkInvitation,2000)}
async function checkInvitation(){
 if(!outgoing||document.hidden)return;
 try{
  const r=await shareApi('transfers/status?token='+encodeURIComponent(outgoing.token));
  if(r.state==='RECEIVED'){
   $('invite-label').textContent='对方已接收';$('cancel-invite').hidden=true;
   shareStatus('光已抵达对方。你们可以开始一起尝试 AI；接收邀请不等于已完成使用。');
   if(!outgoing.received){outgoing.received=true;sessionStorage.setItem('fl_outgoing',JSON.stringify(outgoing));playTransferFX(false,'sender');refreshSharedCount()}
   return;
  }
  if(r.state!=='OFFERED'){$('invite-label').textContent=r.state==='EXPIRED'?'邀请已过期':'邀请已撤销';$('invite-qr').replaceChildren();$('cancel-invite').hidden=true;$('invite').hidden=false;$('invite').textContent='重新生成二维码';return}
 }catch(e){shareStatus(e.message)}
 startPolling();
}
async function refreshSharedCount(){try{const r=await shareApi('me');state.helped=r.received_by_others;render()}catch{}}
async function prepareReceiver(){
 navigate('transfer');$('invite').hidden=true;$('receive-panel').hidden=false;
 try{const r=await shareApi('transfers/status?token='+encodeURIComponent(incoming));
  if(r.is_owner){$('receive-title').textContent='这是你发出的光。';$('accept-invite').hidden=true;shareStatus('请让另一位用户在另一浏览器或设备打开邀请。')}
  else if(r.state==='RECEIVED'){$('receive-title').textContent=r.is_receiver?'这束光已被你接收。':'这份邀请已被接收。';$('accept-invite').hidden=true}
  else if(r.state!=='OFFERED'){$('receive-title').textContent='这份邀请已失效。';$('accept-invite').hidden=true}
  else shareStatus('接收后，发起方会收到消息。此处是本地验收关系，尚未绑定正式账号。');
 }catch(e){shareStatus(e.message);$('accept-invite').disabled=true}
}
$('invite').onclick=createInvite;
$('copy-invite').onclick=async()=>{if(!outgoing)return;try{await navigator.clipboard.writeText(outgoing.url);toast('邀请链接已复制')}catch{shareStatus('无法自动复制。邀请链接：'+outgoing.url)}};
$('cancel-invite').onclick=async()=>{if(!outgoing)return;try{await shareApi('transfers/cancel',{token:outgoing.token});stopPolling();outgoing=null;sessionStorage.removeItem('fl_outgoing');$('invite-panel').hidden=true;$('invite').hidden=false;document.querySelector('.transfer').classList.remove('has-invite');shareStatus('邀请已撤销。')}catch(e){shareStatus(e.message)}};
$('accept-invite').onclick=async()=>{if(!incoming)return;$('accept-invite').disabled=true;try{await shareApi('transfers/accept',{token:incoming});$('receive-title').textContent='这束光，已抵达。';$('accept-invite').hidden=true;shareStatus('邀请已接收，接下来可以请对方陪你尝试 AI。');playTransferFX(false,'receiver')}catch(e){shareStatus(e.message);$('accept-invite').disabled=false}};
$('nfc-info').onclick=()=>$('nfc-dialog').showModal();$('close-nfc').onclick=()=>$('nfc-dialog').close();$('nfc-use-qr').onclick=()=>{$('nfc-dialog').close();if(!outgoing&&!incoming)createInvite()};
function playTransferFX(preview,role='sender'){
 fxPreview=preview;cancelAnimationFrame(fxFrame);
 $('fx-label').textContent=preview?'动效预览 · 未发送真实邀请':role==='receiver'?'接收成功':'对方已接收';
 $('fx-title').textContent='一束光，抵达了。';$('fx-note').textContent=preview?'示意光从一台手机飞向另一台手机。':'已建立邀请接收记录；AI 使用点亮仍待完成。';
 if(!$('transfer-fx').open)$('transfer-fx').showModal();
 const start=performance.now();
 const draw=t=>{if(!$('transfer-fx').open)return;const age=t-start;$('transfer-fx').dataset.phase=age<700?'gather':age<2200?'flight':'arrival';$('fx-title').textContent=age<2200?'让光，去向另一个人。':'一束光，抵达了。';const canvas=$('transfer-canvas'),r=canvas.getBoundingClientRect(),d=Math.min(devicePixelRatio||1,2);if(canvas.width!==Math.round(r.width*d)||canvas.height!==Math.round(r.height*d)){canvas.width=Math.round(r.width*d);canvas.height=Math.round(r.height*d)}const c=canvas.getContext('2d');c.setTransform(d,0,0,d,0,0);FirstLightTransferEffect.draw(c,r.width,r.height,Math.min(t-start,3600),reduced);if(t-start<3600)fxFrame=requestAnimationFrame(draw)};
 fxFrame=requestAnimationFrame(draw);
}
$('preview-transfer').onclick=()=>playTransferFX(true);
$('close-transfer-fx').onclick=()=>$('transfer-fx').close();$('transfer-fx').addEventListener('close',()=>cancelAnimationFrame(fxFrame));
document.addEventListener('visibilitychange',()=>{if(document.hidden)stopPolling();else if(outgoing)startPolling()});
window.addEventListener('pagehide',()=>{stopPolling();cancelAnimationFrame(fxFrame)});
// Personal markers: stored locally, no public publication and no precise location endpoint.
let markerLayer,placing=false,personalMarks=[];
try{const data=JSON.parse(localStorage.getItem('fl_personal_marks')||'[]');if(Array.isArray(data))personalMarks=data.filter(m=>Number.isFinite(m.lat)&&Math.abs(m.lat)<=85&&Number.isFinite(m.lng)&&Math.abs(m.lng)<=180&&typeof m.name==='string').slice(0,100)}catch{}
function saveMarks(){try{localStorage.setItem('fl_personal_marks',JSON.stringify(personalMarks))}catch{toast('浏览器未允许保存标注，刷新后可能丢失。')}}
function renderMarks(){markerLayer.clearLayers();personalMarks.forEach(m=>{const body=document.createElement('div'),title=document.createElement('strong'),note=document.createElement('small'),remove=document.createElement('button');title.textContent=m.name;note.textContent='私人标注 · 仅保存在此浏览器';remove.textContent='删除标注';remove.className='text-button';remove.onclick=()=>{personalMarks=personalMarks.filter(x=>x!==m);saveMarks();renderMarks()};body.append(title,note,remove);L.marker([m.lat,m.lng],{title:m.name,icon:L.divIcon({className:'marked-point',html:'<span></span>',iconSize:[20,20],iconAnchor:[10,10]})}).addTo(markerLayer).bindPopup(body)})}
function enableMapMarks(){
 if(markerLayer)return;markerLayer=L.layerGroup().addTo(realMap);renderMarks();
 realMap.on('click',e=>{if(!placing)return;if(personalMarks.length>=100){toast('最多保存 100 个标注');return}const lng=((e.latlng.lng+180)%360+360)%360-180;personalMarks.push({lat:Math.max(-85,Math.min(85,e.latlng.lat)),lng,name:'我的标注 '+(personalMarks.length+1)});saveMarks();renderMarks();placing=false;$('add-mark').setAttribute('aria-pressed','false');$('add-mark').textContent='＋ 标注地点';$('map-status').textContent='已保存私人标注，点击标注可删除。'});
}
$('add-mark').onclick=()=>{initMap();if(!realMap)return;placing=!placing;$('add-mark').setAttribute('aria-pressed',String(placing));$('add-mark').textContent=placing?'取消标注':'＋ 标注地点';$('map-status').textContent=placing?'请点选地图上的位置。此标注只保存在本浏览器。':'已取消标注。'};
for(const [id,city] of [['world-view','world'],['china-view','china'],['shanghai-view','shanghai']])$(id).onclick=()=>{$('map-city').value=city;resetMap()};
refreshSharedCount();if(incoming)prepareReceiver();else if(outgoing)showQR();

$('retry-map').onclick=()=>{initMap();if(tileLayer)tileLayer.redraw()};
