(function(root){
'use strict';
function createSound({makeAudio=src=>new Audio(src),onChange=()=>{}}={}){
 const intro=makeAudio('audio/opening.mp3'),bgm=makeAudio('audio/bgm.wav');
 intro.preload='none';bgm.preload='none';bgm.loop=true;intro.volume=.72;bgm.volume=.22;
 let enabled=false,phase='idle',generation=0,wasPlaying=false,lastError='';
 const notify=()=>onChange({enabled,phase,error:lastError});
 function stop(){generation++;intro.pause();bgm.pause();wasPlaying=false;phase='idle'}
 async function play(audio,nextPhase){
  const id=++generation;phase=nextPhase;lastError='';
  try{await audio.play();if(id!==generation){if(!enabled)audio.pause();return false}notify();return true}
  catch(error){if(id!==generation)return false;stop();enabled=false;lastError='声音未能播放，请重试或检查浏览器声音权限。';notify();return false}
 }
 function fail(){stop();enabled=false;lastError='音频未能加载，请检查网络后再试。';notify()}
 intro.addEventListener('error',fail);bgm.addEventListener('error',fail);
 return {
  get enabled(){return enabled},get playingIntro(){return enabled&&phase==='intro'&&!intro.paused},
  get currentTime(){return intro.currentTime},
  async enable(inIntro){enabled=true;stop();notify();if(inIntro){intro.currentTime=0;return play(intro,'intro')}bgm.currentTime=0;return play(bgm,'bgm')},
  disable(){enabled=false;stop();lastError='';notify()},
  startIntro(){stop();if(!enabled)return;intro.currentTime=0;return play(intro,'intro')},
  finishIntro(){stop();if(!enabled)return;bgm.currentTime=0;return play(bgm,'bgm')},
  suspend(){const audio=phase==='intro'?intro:bgm;wasPlaying=!audio.paused;audio.pause()},
  resume(){if(!enabled||!wasPlaying)return;wasPlaying=false;return play(phase==='intro'?intro:bgm,phase)},
  dispose(){enabled=false;stop();intro.removeEventListener('error',fail);bgm.removeEventListener('error',fail)}
 };
}
const api={createSound};if(typeof module==='object'&&module.exports)module.exports=api;else root.FirstLightSound=api;
})(typeof window!=='undefined'?window:globalThis);
