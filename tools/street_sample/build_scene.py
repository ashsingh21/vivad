#!/usr/bin/env python3
"""Author a metre-scale Delhi street as real glTF geometry, PBR textures and collision.

No Blender or downloaded art is required. This is an environment asset builder, not an image renderer.
"""
from __future__ import annotations

import json
import math
import random
import struct
from collections import defaultdict
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'assets/models/samples/delhi_street'
TEX = OUT / 'textures'
OUT.mkdir(parents=True, exist_ok=True)
TEX.mkdir(exist_ok=True)
RNG = random.Random(2041)
MATS = []
MESHES = defaultdict(lambda: {'p': [], 'n': [], 'uv': [], 't': [], 'i': []})
COLLIDERS = []


def norm(v):
    a = np.asarray(v, dtype=float)
    return a / max(np.linalg.norm(a), 1e-12)


def frame(origin=(0, 0, 0), yaw=0):
    c, s = math.cos(yaw), math.sin(yaw)
    rotation = np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])
    return np.asarray(origin, dtype=float), rotation


WORLD = frame()


def point(f, p):
    return f[0] + f[1] @ np.asarray(p)


def quad(mat, points, uv=None, normals=None):
    b = MESHES[mat]
    p = np.asarray(points, dtype=float)
    n = norm(np.cross(p[1]-p[0], p[2]-p[0]))
    if np.linalg.norm(n) < .9:
        return
    if uv is None:
        u = norm(p[1]-p[0]); v = norm(np.cross(n, u))
        uv = np.array([[q @ u / 2, -(q @ v) / 2] for q in p])
    else:
        uv = np.asarray(uv)
    dp1, dp2 = p[1]-p[0], p[2]-p[0]
    du1, du2 = uv[1]-uv[0], uv[2]-uv[0]
    den = du1[0]*du2[1]-du1[1]*du2[0]
    tangent = norm((dp1*du2[1]-dp2*du1[1])/den) if abs(den) > 1e-8 else norm(dp1)
    bitangent = norm((-dp1*du2[0]+dp2*du1[0])/den) if abs(den) > 1e-8 else norm(dp2)
    start = len(b['p'])
    for i in range(4):
        nn = n if normals is None else np.asarray(normals[i])
        tt = norm(tangent - nn * np.dot(nn, tangent))
        handedness = 1 if np.dot(np.cross(nn, tt), bitangent) >= 0 else -1
        b['p'].append(p[i].tolist()); b['n'].append(nn.tolist()); b['uv'].append(uv[i].tolist())
        b['t'].append([*tt.tolist(), handedness])
    b['i'].extend([start, start+1, start+2, start, start+2, start+3])


def box(f, center, size, mat, solid=False, name='block'):
    center, size = np.asarray(center), np.asarray(size)
    h = size / 2
    for n, u, v in [( [1,0,0],[0,0,-1],[0,1,0]),([-1,0,0],[0,0,1],[0,1,0]),
                    ([0,1,0],[1,0,0],[0,0,-1]),([0,-1,0],[1,0,0],[0,0,1]),
                    ([0,0,1],[1,0,0],[0,1,0]),([0,0,-1],[-1,0,0],[0,1,0])]:
        n,u,v = map(np.asarray,(n,u,v)); c = center + n*h
        eu,ev=u*abs(u @ h),v*abs(v @ h)
        local=[c-eu-ev,c+eu-ev,c+eu+ev,c-eu+ev]
        world=[point(f,q) for q in local]
        # Metric UVs remain continuous across adjacent modular pieces.
        wu,wv=f[1]@u,f[1]@v
        quad(mat,world, [[q@wu/2,-q@wv/2] for q in world])
    if solid:
        yaw=math.atan2(f[1][0,2],f[1][0,0])
        COLLIDERS.append({'name':name,'center':point(f,center).tolist(),'size':size.tolist(),'rotation':[0,yaw,0]})


def tube(f, a, b, radius, mat, segments=12):
    a,b=np.asarray(a),np.asarray(b); axis=norm(b-a)
    other=np.array([0,1,0]) if abs(axis[1]) < .95 else np.array([1,0,0])
    u=norm(np.cross(axis,other));v=np.cross(axis,u)
    for k in range(segments):
        aa,bb=k*math.tau/segments,(k+1)*math.tau/segments
        d0=math.cos(aa)*u+math.sin(aa)*v;d1=math.cos(bb)*u+math.sin(bb)*v
        q=[a+d0*radius,a+d1*radius,b+d1*radius,b+d0*radius]
        # Test winding against the outward radial direction.
        nn=np.cross(q[1]-q[0],q[2]-q[0])
        if nn @ d0 < 0:q=list(reversed(q)); ds=[d0,d1,d1,d0][::-1]
        else:ds=[d0,d1,d1,d0]
        quad(mat,[point(f,x) for x in q],normals=[f[1]@x for x in ds])


def lathe(f, center, profile, mat, seg=24):
    center=np.asarray(center)
    for (y0,r0),(y1,r1) in zip(profile,profile[1:]):
        if y0==y1 and r0==r1:continue
        for k in range(seg):
            a,b=k*math.tau/seg,(k+1)*math.tau/seg
            d0=np.array([math.cos(a),0,math.sin(a)]);d1=np.array([math.cos(b),0,math.sin(b)])
            q=[center+d0*r0+[0,y0,0],center+d0*r1+[0,y1,0],center+d1*r1+[0,y1,0],center+d1*r0+[0,y0,0]]
            slope=(r1-r0)/max(abs(y1-y0),.0001)
            n0=norm(d0+[0,-slope,0]);n1=norm(d1+[0,-slope,0])
            quad(mat,[point(f,x) for x in q],normals=[f[1]@x for x in [n0,n0,n1,n1]])


def panel(f, center, w, h, mat):
    c=np.asarray(center)
    p=[c+[-w/2,-h/2,0],c+[-w/2,h/2,0],c+[w/2,h/2,0],c+[w/2,-h/2,0]]
    quad(mat,[point(f,x) for x in p],[[1,1],[1,0],[0,0],[0,1]])


def arch(f, width, floor, spring=2.25, top=4.22):
    r=width/2-.22
    for k in range(32):
        a=-math.pi/2+k*math.pi/32;b=-math.pi/2+(k+1)*math.pi/32
        x0,x1=r*math.sin(a),r*math.sin(b)
        y0,y1=floor+spring+r*math.cos(a),floor+spring+r*math.cos(b)
        # Solid spandrel above the genuine arch opening, not a painted arch texture.
        for depth,reverse in [(-.16,False),(.18,True)]:
            q=[[x0,y0,depth],[x0,floor+top,depth],[x1,floor+top,depth],[x1,y1,depth]]
            if reverse:q=q[::-1]
            quad(PLASTER,[point(f,x) for x in q])
        quad(PLASTER,[point(f,x) for x in [[x0,y0,-.16],[x1,y1,-.16],[x1,y1,.18],[x0,y0,.18]]])
        # Raised stone archivolt with a tiny shadow-producing projection.
        for rad0,rad1,depth in [(r-.015,r+.095,-.205)]:
            q=[[rad0*math.sin(a),floor+spring+rad0*math.cos(a),depth],
               [rad1*math.sin(a),floor+spring+rad1*math.cos(a),depth],
               [rad1*math.sin(b),floor+spring+rad1*math.cos(b),depth],
               [rad0*math.sin(b),floor+spring+rad0*math.cos(b),depth]]
            quad(TRIM,[point(f,x) for x in q])


def noise(n, seed, exponent=1.6):
    rng=np.random.default_rng(seed)
    source=rng.normal(size=(n,n))
    ky,kx=np.meshgrid(np.fft.fftfreq(n),np.fft.fftfreq(n),indexing='ij')
    spectrum=(kx*kx+ky*ky+(.8/n)**2)**(-exponent/2)
    result=np.fft.ifft2(np.fft.fft2(source)*spectrum).real
    return (result-result.mean())/result.std()


def texture(name, color, height, rough, metallic=0, tile=2):
    color=np.clip(color,0,255).astype(np.uint8)
    Image.fromarray(color).convert('RGB').save(TEX/f'{name}_base.png')
    n=len(height);pixel=tile/n
    dx=(np.roll(height,-1,1)-np.roll(height,1,1))/(2*pixel)
    dy=(np.roll(height,-1,0)-np.roll(height,1,0))/(2*pixel)
    normal=np.stack([-dx,dy,np.ones_like(dx)],-1);normal/=np.linalg.norm(normal,axis=-1,keepdims=True)
    Image.fromarray(np.uint8(np.clip(normal*.5+.5,0,1)*255)).save(TEX/f'{name}_normal.png')
    mr=np.full((n,n,3),255,dtype=np.uint8);mr[:,:,1]=np.clip(rough,0,1)*255;mr[:,:,2]=np.clip(metallic,0,1)*255
    Image.fromarray(mr).save(TEX/f'{name}_mr.png')


def material(name, color=(1,1,1), rough=.8, metallic=0, tex=None, alpha=None, emissive=None):
    pbr={'baseColorFactor':[*color,alpha if alpha is not None else 1],'roughnessFactor':rough,'metallicFactor':metallic}
    mat={'name':name,'pbrMetallicRoughness':pbr}
    if tex:mat['_texture']=tex
    if alpha is not None:mat['alphaMode']='BLEND';mat['doubleSided']=True
    if emissive:mat['emissiveFactor']=emissive
    MATS.append(mat)
    return len(MATS)-1


def build_textures():
    n=1024; yy,xx=np.mgrid[:n,:n]
    fine=noise(n,15,.1);cloud=noise(n,29,1.7);medium=noise(n,47,.85)
    plaster=211+cloud*2+fine*.8
    texture('plaster',np.stack([plaster,plaster*.985,plaster*.946],-1),cloud*.00035+fine*.00004,.88)
    asphalt=64+fine*3.2+medium*1.8
    stones=(fine>1.7)*8
    texture('asphalt',np.stack([asphalt+stones,asphalt*1.02+stones,asphalt*1.05+stones],-1),fine*.00020+medium*.00014,.87)
    # The renderer uses 2 m repeats. These are 50 x 33 cm stone flags with real mortar joints.
    row=yy//171;px=(xx+(row%2)*128)%256;py=yy%171
    joint=(px<3)|(py<3);pave=172+cloud*2.5+fine*1.3+((xx//256+row)%5-2)*2.2-joint*35
    texture('paving',np.stack([pave,pave*.985,pave*.95],-1),joint*(-.0028)+fine*.00005,.87)
    concrete=148+cloud*3+fine*1.8
    texture('concrete',np.stack([concrete,concrete*.98,concrete*.93],-1),fine*.00010+cloud*.00025,.89)
    wood=103+np.sin(xx*.12+cloud*.4)*5+medium*2
    texture('wood',np.stack([wood,wood*.71,wood*.44],-1),medium*.00015,.72)
    dark=56+medium*2+fine*1.5
    texture('stone',np.stack([dark,dark*1.03,dark*1.02],-1),fine*.00012,.8)
    font='/System/Library/Fonts/Supplemental/Arial.ttf'
    bold='/System/Library/Fonts/Supplemental/Arial Bold.ttf'
    for name,title,subtitle,bg in [
        ('niva','NIVA MARKET','DAILY GOODS  •  SINCE 1998',(23,48,43)),
        ('cafe','AMALTAS CAFE','COFFEE  /  BAKERY',(71,49,36)),
        ('books','PAPER & INK','BOOKS  /  STATIONERY',(33,48,58)),
        ('studio','BLOCK NINE','DESIGN  /  STUDIO',(53,54,52)),
        ('metro','↓  METRO','BLUE CORRIDOR  •  ENTRY',(20,57,83))]:
        im=Image.new('RGB',(1024,256),bg);d=ImageDraw.Draw(im)
        d.rectangle((10,10,1013,245),outline=(154,137,100),width=2)
        d.text((45,35),title,font=ImageFont.truetype(bold,80 if name=='metro' else 74),fill=(235,229,210))
        d.text((49,155),subtitle,font=ImageFont.truetype(font,25),fill=(177,181,166))
        im.save(TEX/f'{name}_sign.png')


build_textures()
PLASTER=material('aged ivory plaster',tex='plaster')
TRIM=material('painted stone mouldings',(.9,.885,.83),.8)
CONCRETE=material('precast concrete',tex='concrete')
ASPHALT=material('fine asphalt',tex='asphalt')
PAVING=material('warm paving flags',tex='paving')
STONE=material('dark stone bases',tex='stone')
WOOD=material('shop joinery',tex='wood')
IRON=material('painted charcoal metal',(.055,.069,.064),.48,.45)
ALUMINUM=material('brushed aluminium',(.45,.49,.47),.42,.8)
GLASS=material('shop glass',(.12,.2,.18),.1,.2,alpha=.17)
WINDOW=material('upper glazing',(.025,.065,.095),.13,.65)
RUBBER=material('rubber',(.025,.03,.027),.92)
ROADPAINT=material('road markings',(.72,.73,.66),.9)
YELLOW=material('curb safety yellow',(.63,.44,.105),.82)
SOIL=material('planter soil',(.085,.068,.045),1)
WARM=material('warm lamp diffuser',(.82,.68,.43),.5,emissive=[.65,.45,.2])
WHITE=material('car pearl paint',(.69,.73,.72),.29,.28)
BLUECAR=material('car slate paint',(.15,.25,.29),.27,.4)
TANCAR=material('car muted bronze paint',(.35,.26,.15),.31,.4)
LIGHT=material('headlamp glass',(.77,.8,.73),.15,.25)
RED=material('tail lamp',(.32,.025,.012),.27,.1)
BLUE=material('metro enamel blue',(.035,.12,.19),.5,.25)
SHOPMATS=[]
for name in ['niva','cafe','books','studio','metro']:
    idx=material(name+' sign',rough=.65);MATS[idx]['_sign']=name;SHOPMATS.append(idx)
PRODUCTS=[material('stock '+str(i),c,.8) for i,c in enumerate([(.3,.16,.08),(.5,.38,.16),(.2,.29,.26),(.65,.63,.48),(.32,.12,.09)])]


def curve(z):
    return .00065*min(0,z-30)**2


def row_frame(side,z):
    a=math.atan(.0013*min(0,z-30))
    return frame((curve(z)+side*8.9,.18,z),side*math.pi/2+a)


def collision_box(center,size,rotation=(0,0,0),name='support'):
    COLLIDERS.append({'name':name,'center':list(center),'size':list(size),'rotation':list(rotation)})


def streets():
    # Four foundation rectangles leave an actual opening around the underground entrance.
    for lo,hi in [((-220,-220),(5.8,220)),((13.8,-220),(220,220)),((5.8,-220),(13.8,-2)),((5.8,16),(13.8,220))]:
        x,z=(lo[0]+hi[0])/2,(lo[1]+hi[1])/2
        box(WORLD,(x,-.3,z),(hi[0]-lo[0],.5,hi[1]-lo[1]),CONCRETE,True,'foundation')
    for z in np.arange(-100,70,1.5):
        zz=[z,z+1.5]
        q=[[curve(t)+off,.002,t] for t,off in [(zz[0],-5.6),(zz[0],5.6),(zz[1],5.6),(zz[1],-5.6)]]
        quad(ASPHALT,q[::-1])
        for side in [-1,1]:
            if side==1 and -2<z<16:continue
            f=frame((curve(z)+side*7.25,.06,z+.75))
            box(f,(0,0,0),(3.3,.24,1.52),PAVING)
            collision_box((curve(z)+side*7.25,.06,z+.75),(3.3,.24,1.52),name='pavement')
            box(WORLD,(curve(z)+side*5.7,.04,z+.75),(.18,.2,1.5),CONCRETE)
    # Dashed centre line is restrained; parked cars sit outside the 7 m through carriageway.
    for z in np.arange(-95,65,7):
        box(WORLD,(curve(z),.008,z),(.09,.012,2.8),ROADPAINT)
    for x in np.arange(-4.9,5.2,1.25):
        box(WORLD,(curve(30)+x,.013,30),(.65,.018,3.0),ROADPAINT)
    for side,z in [(-1,24),(1,25),(-1,-6),(1,-24)]:
        f=frame((curve(z)+side*5.5,.025,z))
        box(f,(0,0,0),(.45,.04,.65),IRON)
        for k in range(8):box(f,(0,.024,-.29+k*.083),(.4,.015,.015),ALUMINUM)


def shop(f,index):
    # Real room volume behind a framed, open doorway. Walkable arcade remains 3.1 m deep.
    box(f,(0,.02,4.0),(4.22,.16,8.0),PAVING,True,'arcade floor')
    box(f,(0,3.97,5.6),(4.22,.14,4.8),PLASTER)
    box(f,(0,1.9,8.0),(4.22,3.8,.2),PLASTER,True,'shop rear wall')
    for x in [-2.04,2.04]:box(f,(x,1.95,5.7),(.14,3.9,4.8),PLASTER,True,'party wall')
    for x in [-1.25,1.25]:box(f,(x,.29,3.22),(1.14,.38,.25),STONE)
    box(f,(0,3.13,3.22),(4.1,.82,.25),WOOD)
    shutter=index%6==2
    if shutter:
        box(f,(0,1.58,3.22),(3.72,2.4,.08),ALUMINUM,True,'closed shop shutter')
        for k in range(30):box(f,(0,.46+k*.076,3.15),(3.7,.018,.022),IRON)
    else:
        for x in [-1.25,1.25]:
            for edge in [-.56,.56]:box(f,(x+edge,1.65,3.19),(.06,2.35,.09),WOOD)
            for y in [.475,2.825]:box(f,(x,y,3.19),(1.12,.07,.09),WOOD)
            panel(f,(x,1.65,3.13),1.0,2.2,GLASS)
        for x in [-1.87,-.65,.65,1.87]:box(f,(x,1.62,3.11),(.055,2.5,.06),ALUMINUM)
        # Window pane side geometry is visual only; wall colliders leave a genuine 1.3 m doorway.
        for x in [-1.26,1.26]:
            collision_box(point(f,(x,1.6,3.2)),(1.16,2.8,.15),(0,math.atan2(f[1][0,2],f[1][0,0]),0),'display window')
        box(f,(0,.5,6.6),(2.8,.9,.55),WOOD,True,'retail counter')
        for y in [.8,1.5,2.2]:
            box(f,(0,y,7.65),(3.2,.07,.48),WOOD)
            for k in range(9):
                box(f,(-1.35+k*.33,y+.17,7.58),(.19,.26+RNG.random()*.15,.23),PRODUCTS[(k+index)%len(PRODUCTS)])
        for x in [-1.55,1.55]:box(f,(x,1.25,7.62),(.07,2.5,.5),IRON)
        box(f,(0,3.8,5.1),(1.1,.07,.17),WARM)
        # Small product displays sit at the shop edge while preserving the arcade's walking lane.
        for xx in [-1.55,1.55]:
            box(f,(xx,.42,2.72),(.6,.7,.52),WOOD)
            for k in range(3):box(f,(xx-.2+k*.2,.85,2.72),(.16,.18,.33),PRODUCTS[(k+index)%len(PRODUCTS)])
    sign=SHOPMATS[0 if index in [3,4] else [0,3,1,3,2,3][index%6]]
    box(f,(0,3.04,3.02),(3.8,.78,.1),WOOD)
    panel(f,(0,3.05,2.95),3.72,.74,sign)


def architecture():
    for side in [-1,1]:
        for i in range(25):
            z=40-i*4.2
            if side==1 and -2<z<18:continue
            f=row_frame(side,z)
            shop(f,i+(0 if side<0 else 2))
            # Walkable first floor / balcony and ceiling.
            box(f,(0,4.18,4.0),(4.24,.24,8.0),TRIM,True,'first floor')
            box(f,(0,8.42,4.0),(4.24,.24,8.0),TRIM)
            box(f,(0,6.35,8),(4.22,4.2,.2),PLASTER)
            box(f,(0,6.05,3.3),(4.1,3.4,.2),PLASTER)
            for x in [-1.1,1.1]:
                box(f,(x,6.05,3.16),(1.35,2.1,.12),WOOD)
                box(f,(x,6.05,3.08),(1.16,1.91,.05),WINDOW)
                box(f,(x,6.05,3.02),(.055,1.95,.06),TRIM)
                box(f,(x,6.05,3.02),(1.18,.05,.06),TRIM)
                box(f,(x,4.98,3.03),(1.55,.11,.3),TRIM)
            for floor in [0,4.24]:
                arch(f,4.2,floor)
                columns=[-2.1]
                if (side<0 and i==0) or (side>0 and i in [5,24]):columns.append(2.1)
                for x in columns:
                    lathe(f,(x,floor,0),[(0,.29),(.1,.29),(.16,.25),(.24,.2),(1.93,.18),(2.0,.22),(2.05,.28),(2.18,.28),(2.24,.31)],PLASTER)
                    if floor==0:
                        collision_box(point(f,(x,1.08,0)),(.43,2.16,.43),
                                      (0,math.atan2(f[1][0,2],f[1][0,0]),0),'arcade column')
                # Simple cornice profile, with actual overhangs that cast contact shadows.
                for yy,depth,height in [(4.12,.55,.1),(4.23,.7,.09),(4.31,.8,.08)]:
                    box(f,(0,floor+yy,-.03),(4.25,height,depth),TRIM)
            # Open iron balustrade on the upper arcade.
            for x in np.arange(-1.93,2,.21):tube(f,(x,4.42,-.1),(x,5.27,-.1),.016,IRON,8)
            for y in [4.44,5.28]:tube(f,(-2.05,y,-.1),(2.05,y,-.1),.025,IRON,10)
            box(f,(0,8.89,.1),(4.22,.65,.28),PLASTER)
            box(f,(0,9.25,.1),(4.34,.09,.44),TRIM)
            if i%4==0:
                box(f,(1.0,7.4,2.89),(.85,.55,.38),TRIM)
                for k in range(6):box(f,(.68+k*.125,7.4,2.68),(.045,.37,.045),IRON)
                lathe(f,(-.9,8.58,5.6),[(0,.6),(.12,.68),(1.05,.68),(1.18,.55)],IRON)
                tube(f,(1.6,8.5,6.5),(1.6,10,6.5),.025,IRON)
            # Small drain pipes establish scale without covering every facade in repeated clutter.
            if i%3==0:tube(f,(1.85,.2,3.0),(1.85,8.9,3.0),.04,IRON)


def car(z,side,paint):
    f=frame((curve(z)+side*4.45,.0,z),0 if side<0 else math.pi)
    sections=[(-2.12,.58,.5,.73),(-1.95,.8,.38,.87),(-1.1,.88,.36,1.03),
              (-.48,.8,.36,1.55),(.63,.8,.36,1.52),(1.28,.86,.36,.95),(1.95,.77,.46,.81),(2.1,.56,.52,.72)]
    # Interpolate body stations before smoothing the sheet-metal surface.
    stations=[]
    original=np.asarray(sections,dtype=float)
    for i in range(len(original)-1):
        p0,p1,p2,p3=original[max(0,i-1)],original[i],original[i+1],original[min(len(original)-1,i+2)]
        for t in np.linspace(0,1,4,endpoint=False):
            q=.5*((2*p1)+(-p0+p2)*t+(2*p0-5*p1+4*p2-p3)*t*t+(-p0+3*p1-3*p2+p3)*t*t*t)
            stations.append(q)
    stations.append(original[-1])
    body=[]
    for a,b in zip(stations,stations[1:]):
        z0,w0,bot0,top0=a;z1,w1,bot1,top1=b
        loops=[]
        for zz,w,lo,hi in [a,b]:
            loops.append([[-w*.85,lo,zz],[-w,lo+.18,zz],[-w,hi-.12,zz],[-w*.83,hi,zz],[w*.83,hi,zz],[w,hi-.12,zz],[w,lo+.18,zz],[w*.85,lo,zz]])
        for i in range(8):body.append([point(f,p) for p in [loops[0][i],loops[1][i],loops[1][(i+1)%8],loops[0][(i+1)%8]]])
    normals=defaultdict(lambda:np.zeros(3))
    key=lambda p:tuple(np.round(p,5))
    for q in body:
        n=norm(np.cross(q[1]-q[0],q[2]-q[0]))
        for p in q:normals[key(p)]+=n
    for q in body:quad(paint,q,normals=[norm(normals[key(p)]) for p in q])
    # Opaque coated glazing remains affordable and reflects the actual environment light.
    for s in [-1,1]:
        points=[[s*.895,1.04,-1.0],[s*.79,1.50,-.43],[s*.79,1.48,.56],[s*.895,1.02,1.16]]
        if s<0:points=points[::-1]
        quad(WINDOW,[point(f,p) for p in points])
        tube(f,(s*.82,1.05,.1),(s*.75,1.46,.1),.025,IRON)
        box(f,(s*.87,.85,.72),(.035,.055,.22),ALUMINUM)
        box(f,(s*.93,1.07,.97),(.17,.11,.2),paint)
    for zz,topzz,baseh,toph in [(-1.04,-.48,1.1,1.59),(1.27,.64,1.04,1.56)]:
        q=[[-.72,baseh,zz],[-.66,toph,topzz],[.66,toph,topzz],[.72,baseh,zz]]
        if zz>0:q=q[::-1]
        quad(WINDOW,[point(f,p) for p in q])
    for s in [-1,1]:
        for zz in [-1.3,1.28]:
            wf=frame(point(f,(s*.85,.34,zz)),math.atan2(f[1][0,2],f[1][0,0]))
            # Rotate the wheel's lathe axis from Y to X.
            rot=np.array([[0,1,0],[-1,0,0],[0,0,1]])
            wf=(wf[0],wf[1]@rot)
            lathe(wf,(0,0,0),[(-.12,.24),(-.1,.31),(.08,.32),(.13,.26)],RUBBER,24)
            lathe(wf,(0,0,0),[(.135,.0),(.136,.19),(.15,.2)],ALUMINUM,16)
            for k in range(8):
                a=k*math.tau/8
                tube(wf,(0,.155,0),(.16*math.cos(a),.155,.16*math.sin(a)),.015,IRON,6)
    for s in [-1,1]:
        box(f,(s*.53,.73,1.99),(.35,.15,.04),LIGHT)
        box(f,(s*.58,.75,-1.98),(.22,.18,.04),RED)
    for zz in [-2.12,2.1]:box(f,(0,.62,zz),(1.16,.23,.035),paint)
    box(f,(0,.57,2.06),(.82,.12,.04),IRON)
    box(f,(0,.6,-2.05),(.47,.12,.04),TRIM)
    collision_box(f[0]+[0,.8,0],(1.85,1.6,4.25),(0,0,0),'parked car')


def street_furniture():
    for side,z in [(-1,36),(1,29),(-1,9),(1,-12),(-1,-32),(1,-47)]:
        x=curve(z)+side*7.25
        f=frame((x,.18,z))
        lathe(f,(0,0,0),[(0,.2),(.15,.2),(.23,.09),(4.75,.065)],IRON,16)
        tube(f,(0,4.7,0),(side*-.3,5.03,0),.065,IRON)
        tube(f,(side*-.3,5.03,0),(side*-.85,5.03,0),.06,IRON)
        lathe(f,(side*-.85,4.76,0),[(0,.26),(.06,.26),(.16,.18),(.24,.06)],IRON,24)
        box(f,(side*-.85,4.75,0),(.28,.025,.28),WARM)
    for side,z in [(-1,26),(-1,-13),(1,-20),(1,38)]:
        f=frame((curve(z)+side*7.2,.18,z))
        box(f,(0,.32,0),(1.15,.64,1.8),CONCRETE,True,'planter')
        box(f,(0,.655,0),(1.0,.03,1.65),SOIL)
        for axis in [-1,1]:box(f,(axis*.53,.69,0),(.12,.12,1.94),TRIM)
        for axis in [-1,1]:box(f,(0,.69,axis*.87),(1.18,.12,.12),TRIM)
    for z in [37,18,-5,-29]:
        for side in [-1,1]:
            f=frame((curve(z)+side*6.35,.18,z))
            lathe(f,(0,0,0),[(0,.095),(.68,.065),(.73,.055)],IRON,12)
    for z in [20,-17]:
        f=frame((curve(z)-7.15,.18,z))
        for yy in [.43,.5]:
            for zz in [-.22,0,.22]:box(f,(0,yy,zz),(1.9,.04,.12),WOOD)
        for xx in [-.76,.76]:box(f,(xx,.23,0),(.09,.44,.54),IRON)
        box(f,(0,.8,.26),(1.9,.48,.06),WOOD)


def metro():
    # The opening has no ground collider across it; the stairs lead to a real small concourse.
    x=8.2
    box(WORLD,(x,.06,15.4),(4.4,.24,2.4),PAVING,True,'metro upper landing')
    box(WORLD,(x,-3.85,-1.2),(5.0,.3,7.0),PAVING,True,'metro lower landing')
    for s in [-1,1]:box(WORLD,(x+s*2.24,-1.8,7),(.25,4.2,18),CONCRETE,True,'metro retaining wall')
    for step in range(27):
        z=14-step*.48;y=.18-step*(3.9/27)
        box(WORLD,(x,y-.08,z),(4.25,.16,.49),CONCRETE)
        box(WORLD,(x,y+.005,z+.19),(4.22,.012,.035),ROADPAINT)
    # A simple swept ramp collider supports movement on the visual treads.
    angle=math.atan2(3.9,12.96)
    collision_box((x,-1.88,7.76),(4.25,.25,math.hypot(3.9,12.96)),(-angle,0,0),'metro stair ramp')
    for side in [-1,1]:
        for zz in [14.3,10.2,6.1,2.0]:
            yy=.18-(14-zz)*(3.9/12.96)
            tube(WORLD,(x+side*1.9,yy,zz),(x+side*1.9,yy+1.05,zz),.035,ALUMINUM)
        tube(WORLD,(x+side*1.9,1.25,14.3),(x+side*1.9,-2.65,1.34),.04,ALUMINUM)
    for side in [-1,1]:
        tube(WORLD,(x+side*2.18,.2,14.7),(x+side*2.18,3.1,14.7),.08,IRON)
        tube(WORLD,(x+side*2.18,.2,2),(x+side*2.18,2.7,2),.08,IRON)
    box(WORLD,(x,3.12,9.0),(4.8,.17,12.8),IRON)
    box(WORLD,(x,2.72,15.2),(4.8,.76,.22),BLUE)
    # Front-facing sign uses a frame whose normal points toward +Z.
    f=frame((x,2.72,15.34),math.pi)
    panel(f,(0,0,0),4.62,.72,SHOPMATS[4])
    box(WORLD,(x,-2.1,-4.2),(4.6,3.5,.18),BLUE,True,'metro end wall')
    panel(frame((x,-1.7,-4.05),math.pi),(0,0,0),3.6,.65,SHOPMATS[4])
    for zz in [11,5]:box(WORLD,(x,2.9,zz),(2.0,.06,.16),WARM)


def background():
    # Modest off-axis skyline, without importing the entire 4 km map.
    for i,(x,z,h) in enumerate([(-25,-128,25),(-2,-153,45),(22,-141,35),(44,-168,58),(-57,-159,36),(65,-190,44)]):
        f=frame((x,0,z))
        box(f,(0,h/2,0),(13,h,15),CONCRETE)
        for side in [-1,1]:
            box(f,(0,h/2,side*7.53),(12.4,h-1,.08),WINDOW)
            for y in np.arange(3,h,3.4):box(f,(0,y,side*7.59),(12.8,.15,.15),TRIM)
            for xx in np.arange(-5.5,6,2.3):box(f,(xx,h/2,side*7.62),(.11,h,.12),ALUMINUM)
        box(f,(0,h+.7,0),(10,1.4,12),IRON)
    # End-of-street building closes the view at a believable junction rather than a void.
    for i in range(-4,5):
        f=frame((i*4.25+curve(-75),0,-82),0)
        box(f,(0,5.0,0),(4.2,10,6),PLASTER)
        for y in [1.8,5.0,8]:
            box(f,(0,y,3.04),(2.6,1.9,.08),WOOD)
            box(f,(0,y,3.1),(2.35,1.65,.06),WINDOW)
    # A few rooftop service cables form relaxed, believable curves.
    for side in [-1,1]:
        for za,zb in [(30,13),(0,-19),(-30,-48)]:
            last=None
            for t in np.linspace(0,1,12):
                z=za+(zb-za)*t;p=(curve(z)+side*10.8,7.7-.5*math.sin(math.pi*t),z)
                if last is not None:tube(WORLD,last,p,.012,IRON,6)
                last=p


streets();architecture();street_furniture();metro();background()
for z,side,paint in [(23,-1,WHITE),(7,-1,BLUECAR),(-21,-1,TANCAR),(30,1,BLUECAR),(-16,1,WHITE),(-44,1,WHITE)]:car(z,side,paint)


def export():
    binary=bytearray();views=[];accessors=[]
    def array(values,kind,components,ctype):
        values=np.asarray(values,dtype=np.float32 if ctype==5126 else np.uint32)
        while len(binary)%4:binary.append(0)
        offset=len(binary);raw=values.tobytes();binary.extend(raw)
        views.append({'buffer':0,'byteOffset':offset,'byteLength':len(raw),'target':34963 if kind=='indices' else 34962})
        a={'bufferView':len(views)-1,'componentType':ctype,'count':len(values),'type':components}
        if kind=='positions':a.update(min=values.min(axis=0).tolist(),max=values.max(axis=0).tolist())
        accessors.append(a);return len(accessors)-1
    images=[];textures=[];materials=[]
    def image(path):
        images.append({'uri':'textures/'+path});textures.append({'sampler':0,'source':len(images)-1});return len(textures)-1
    for original in MATS:
        mat={k:v for k,v in original.items() if not k.startswith('_')};mat['pbrMetallicRoughness']=dict(original['pbrMetallicRoughness'])
        if '_texture' in original:
            name=original['_texture']
            vendor_path=TEX/'vendor/sources.json'
            vendor=json.loads(vendor_path.read_text()).get('assets',{}).get(name) if vendor_path.exists() else None
            if vendor:
                maps=vendor['maps'];mr='arm' if 'arm' in maps else 'rough'
                mat['pbrMetallicRoughness'].update(baseColorTexture={'index':image('vendor/'+maps['diff']['file'])},metallicRoughnessTexture={'index':image('vendor/'+maps[mr]['file'])})
                mat['normalTexture']={'index':image('vendor/'+maps['nor_gl']['file']),'scale':.65}
            else:
                mat['pbrMetallicRoughness'].update(baseColorTexture={'index':image(name+'_base.png')},metallicRoughnessTexture={'index':image(name+'_mr.png')})
                mat['normalTexture']={'index':image(name+'_normal.png'),'scale':1}
            mat['pbrMetallicRoughness']['roughnessFactor']=1
            mat['pbrMetallicRoughness']['metallicFactor']=0 if vendor else 1
        if '_sign' in original:mat['pbrMetallicRoughness']['baseColorTexture']={'index':image(original['_sign']+'_sign.png')}
        materials.append(mat)
    meshes=[];nodes=[];triangles=0;vertices=0
    for mat,b in sorted(MESHES.items()):
        attributes={'POSITION':array(b['p'],'positions','VEC3',5126),'NORMAL':array(b['n'],'normals','VEC3',5126),
                    'TEXCOORD_0':array(b['uv'],'uv','VEC2',5126),'TANGENT':array(b['t'],'tangent','VEC4',5126)}
        indices=array(b['i'],'indices','SCALAR',5125)
        meshes.append({'name':materials[mat]['name'],'primitives':[{'attributes':attributes,'indices':indices,'material':mat}]})
        nodes.append({'name':materials[mat]['name'],'mesh':len(meshes)-1})
        triangles+=len(b['i'])//3;vertices+=len(b['p'])
    doc={'asset':{'version':'2.0','generator':'Vivad original Delhi street asset builder'},'scene':0,
         'scenes':[{'name':'Delhi street sample','nodes':list(range(len(nodes)))}], 'nodes':nodes,'meshes':meshes,
         'materials':materials,'images':images,'textures':textures,
         'samplers':[{'magFilter':9729,'minFilter':9987,'wrapS':10497,'wrapT':10497}],
         'buffers':[{'uri':'street.bin','byteLength':len(binary)}],'bufferViews':views,'accessors':accessors}
    (OUT/'street.bin').write_bytes(binary)
    (OUT/'street.gltf').write_text(json.dumps(doc,separators=(',',':')))
    props=[]
    for side,z in [(-1,5),(1,-12),(-1,-32),(1,-44)]:
        props.append({'model':'models/props/tree_neem.gltf','position':[curve(z)+side*7.1,.18,z],'scale':1.05})
    for side,z in [(-1,26),(-1,-13),(1,-20),(1,38)]:
        props.append({'model':'models/props/bush_lantana.gltf','position':[curve(z)+side*7.2,.85,z],'scale':.35})
    def arr(a):return '('+','.join(f'{v:.6f}' for v in a)+')'
    ron='(colliders:[\n'+''.join(f'(name:{json.dumps(c["name"])},center:{arr(c["center"])},size:{arr(c["size"])},rotation:{arr(c["rotation"])}),\n' for c in COLLIDERS)+'],props:[\n'
    ron+=''.join(f'(model:{json.dumps(p["model"])},position:{arr(p["position"])},scale:{p["scale"]}),\n' for p in props)+'])\n'
    (OUT/'collision.ron').write_text(ron)
    report={'triangles':triangles,'vertices':vertices,'material_batches':len(meshes),'colliders':len(COLLIDERS),
            'building_bays':sum(1 for side in [-1,1] for i in range(25) if not (side==1 and -2<40-i*4.2<18)),
            'street_length_m':170,'units':'metres','geometry':'true arches, profiled columns, open shop rooms, layered balconies, metro excavation and authored cars',
            'provenance':'Original generated geometry, signs and procedural textures; Poly Haven CC0 surface maps where available; existing repo vegetation is instanced by the sample viewer.',
            'external_surface_manifest':'textures/vendor/sources.json' if (TEX/'vendor/sources.json').exists() else None}
    (OUT/'asset-report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


if __name__=='__main__':export()
