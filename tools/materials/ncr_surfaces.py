"""Reproducible, original tiling PBR maps for NCR. No downloaded source imagery.
Metres per tile are assigned in the renderer; RGB MR uses green=roughness, blue=metalness.
"""
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw, ImageFont
OUT=Path(__file__).resolve().parents[2]/'assets/textures/ncr'
OUT.mkdir(parents=True,exist_ok=True)
N=512
rng=np.random.default_rng(1047)
y,x=np.mgrid[0:N,0:N]
fine=rng.normal(0,1,(N,N))
cloud=sum(np.sin(2*np.pi*(x*fx+y*fy)/N+phase)*amp for fx,fy,phase,amp in [(2,1,.2,.5),(3,-2,2,.3),(7,5,1,.12),(13,-9,0,.08)])
def write(name,color,height,rough,metal=0):
    rgb=np.clip(color,0,255).astype('uint8')
    if rgb.ndim==2:rgb=np.repeat(rgb[:,:,None],3,axis=2)
    Image.fromarray(rgb).convert('RGBA').save(OUT/f'{name}_basecolor.png')
    dx=(np.roll(height,-1,1)-np.roll(height,1,1))*2
    dy=(np.roll(height,-1,0)-np.roll(height,1,0))*2
    v=np.stack([-dx,dy,np.ones_like(dx)],-1);v/=np.linalg.norm(v,axis=-1,keepdims=True)
    Image.fromarray(np.uint8((v*.5+.5)*255)).convert('RGBA').save(OUT/f'{name}_normal.png')
    mr=np.zeros((N,N,4),dtype='uint8');mr[:,:,0]=255;mr[:,:,1]=np.clip(rough,0,1)*255;mr[:,:,2]=metal*255;mr[:,:,3]=255
    Image.fromarray(mr).save(OUT/f'{name}_mr.png')
noise=fine*4+cloud*1.3
asphalt=np.stack([np.full_like(noise,58),np.full_like(noise,62),np.full_like(noise,64)],-1)+noise[:,:,None]
# Scattered aggregate, rather than a uniformly black road.
aggregate=rng.random((N,N))>.96
asphalt+=aggregate[:,:,None]*18
write('asphalt',asphalt,fine*.025+aggregate*.08,.82+fine*.025)
concrete=170+fine*2.5+cloud*3
pits=rng.random((N,N))>.992
concrete-=pits*28
write('concrete',np.stack([concrete,concrete*.985,concrete*.95],-1),fine*.012-pits*.1,.84+cloud*.035)
# Offset 50 x 25 cm paving, with fine sand in joints.
row=y//64;px=(x+(row%2)*64)%128;py=y%64
joint=(px<3)|(py<3)
variation=((x//128+row*3)%5-2)*3
paver=154+fine*2+cloud*5+variation-joint*36
write('pavers',np.stack([paver,paver*.98,paver*.93],-1),fine*.009-joint*.11,.82+joint*.1)
ground=204+fine*4+cloud*3
write('ground',np.stack([ground,ground*.98,ground*.94],-1),fine*.03,.93)
# A quiet, slightly varied coated curtain-wall panel. Reflections come from the sky IBL.
col=cloud*.35+fine*.1
write('glass',np.stack([45+col,69+col,79+col],-1),np.zeros_like(cloud),np.full_like(cloud,.23),.45)
metal=149+fine*2+np.sin(y*.6)*2
write('steel',np.stack([metal*.92,metal*.97,metal],-1),fine*.006,.38+cloud*.06,.8)
font='/System/Library/Fonts/Supplemental/Arial Bold.ttf'
for name,title,sub,bg in [('shop','NIVA MARKET','DAILY GOODS  /  OPEN 07–22',(26,58,53)),('metro','METRO','BLUE LINE   /   ENTRY  →',(20,71,102))]:
    im=Image.new('RGBA',(1024,256),bg+(255,));d=ImageDraw.Draw(im)
    d.rectangle((0,0,12,255),fill=(217,184,112));d.text((48,27),title,font=ImageFont.truetype(font,91),fill=(236,233,216));d.text((52,160),sub,font=ImageFont.truetype(font,31),fill=(182,202,198));im.save(OUT/f'{name}_basecolor.png')
print(f'Wrote six PBR sets and two signs to {OUT}')
