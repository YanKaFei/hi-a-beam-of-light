import math,wave,array,random,json
from pathlib import Path
SR=32000
OUT=Path(__file__).resolve().parent
OUT.mkdir(parents=True,exist_ok=True)
random.seed(16)
score=[]
def hz(m):return 440*2**((m-69)/12)
def make(duration,loop=False):
 n=int(duration*SR);left=array.array('f',[0])*n;right=array.array('f',[0])*n
 def add(at,dur,midi,amp=.08,kind='key',pan=0):
  f=hz(midi);beg=int(at*SR);length=int(dur*SR)
  score.append(dict(track='bgm' if loop else 'opening',at=at,duration=dur,note=midi,kind=kind))
  for j in range(length):
   t=j/SR
   if kind=='pad':
    env=min(1,t/1.8)*min(1,(dur-t)/2);tone=math.sin(2*math.pi*f*t)+.16*math.sin(2*math.pi*(f*2+.16)*t)
   elif kind=='bell':
    env=(1-math.exp(-t*90))*math.exp(-t*2.6)*min(1,(dur-t)/.12);tone=math.sin(2*math.pi*f*t)+.2*math.sin(2*math.pi*f*2.003*t)*math.exp(-t*2)
   else:
    env=(1-math.exp(-t*110))*math.exp(-t*1.05)*min(1,(dur-t)/.15);tone=math.sin(2*math.pi*f*t)+.24*math.sin(2*math.pi*f*2*t)*math.exp(-t*1.6)+.07*math.sin(2*math.pi*f*3*t)*math.exp(-t*2.4)
   value=amp*tone*env
   idx=beg+j
   if loop:idx%=n
   if 0<=idx<n:left[idx]+=value*math.sqrt((1-pan)/2);right[idx]+=value*math.sqrt((1+pan)/2)
 # Warm Cmaj9 -> Am9 -> Fmaj9 -> Gsus, open voicings, 60 BPM.
 chords=[[48,55,59,62,64],[45,52,55,59,60],[41,48,55,57,64],[43,50,55,57,62]]
 spacing=8 if loop else 4
 for k,ch in enumerate(chords):
  start=k*spacing
  for j,note in enumerate(ch):add(start,spacing+2,note,.023 if loop else .031,'pad',(j-2)*.16)
 if loop:
  motif=[(0,72),(2.2,76),(5,79),(8.5,71),(11,72),(15,76),(16.5,69),(20,72),(23,76),(25,74),(28,71)]
  for t,m in motif:add(t,4.5,m,.065,'key',.22*math.sin(t))
 else:
  for t,m in [(0.25,72),(1.45,76),(2.8,79),(4.75,72),(6.9,79),(8.15,76),(10.1,69),(11.2,72),(12.35,76),(14.6,72),(15.3,79)]:add(t,3.2,m,.09 if t<9 else .065,'key',-.15 if m==72 else .18)
  # Audible consequences: gather, arrival, then a modest release at the welcome.
  for t,m,a in [(4.75,84,.04),(6.9,88,.055),(9.65,84,.028),(10.1,88,.023),(14.6,79,.045)]:add(t,2,m,a,'bell',.28 if t==6.9 else -.2)
 # Short stereo reflections, wrapped only for the seamless loop.
 origL=array.array('f',left);origR=array.array('f',right)
 for delay,gain in [(.17,.13),(.31,.09),(.53,.055)]:
  offset=int(delay*SR)
  for i in range(n):
   target=i+offset
   if loop:target%=n
   if target<n:left[target]+=origR[i]*gain;right[target]+=origL[i]*gain
 peak=max(max(abs(x) for x in left),max(abs(x) for x in right));gain=.40/max(peak,.0001)
 pcm=array.array('h')
 for i in range(n):
  fade=1 if loop else min(1,i/(SR*.6),(n-1-i)/(SR*1.5))
  pcm.extend((int(max(-1,min(1,left[i]*gain*fade))*32767),int(max(-1,min(1,right[i]*gain*fade))*32767)))
 path=OUT/('bgm.wav' if loop else 'opening.wav')
 with wave.open(str(path),'wb') as f:f.setparams((2,2,SR,n,'NONE','not compressed'));f.writeframes(pcm.tobytes())
 print(path, 'seconds',duration,'peak',round(peak*gain,3))
make(17.6);make(32,True)
(OUT/'score.json').write_text(json.dumps(score,ensure_ascii=False,indent=2))
