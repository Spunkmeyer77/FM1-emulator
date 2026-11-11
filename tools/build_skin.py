from PIL import Image
import numpy as np
from scipy import ndimage
OUT='/home/claude/fm1-emulator/crates/fm1-plugin/skin/'
import os; os.makedirs(OUT,exist_ok=True)
def objs(alpha,minsz=40):
    lab,n=ndimage.label(alpha>20)
    r=[]
    for s in ndimage.find_objects(lab):
        if s[0].stop-s[0].start>minsz and s[1].stop-s[1].start>minsz: r.append(s)
    return sorted(r,key=lambda s:(s[0].start//100,s[1].start))
def white_to_alpha(img):
    a=np.asarray(img.convert('RGB')).astype(float)
    al=np.clip((255-a.min(2))/255*1.0,0,1)
    al=np.where(al<0.03,0,al)
    col=np.where(al[...,None]>0,(a-255*(1-al[...,None]))/np.maximum(al[...,None],1e-3),0)
    o=np.dstack([np.clip(col,0,255),al*255]).astype(np.uint8)
    return Image.fromarray(o,'RGBA')
def save(img,name): img.save(OUT+name+'.png',optimize=True)
# --- sprites: pressed keys from sprites.png (row 2: cream, brown pressed)
sp=Image.open('sprites.png'); sa=np.asarray(sp)[...,3]
so=objs(sa); print('sprites',[(s[1].start,s[0].start,s[1].stop-s[1].start,s[0].stop-s[0].start) for s in so])
crop=lambda im,s: im.crop((s[1].start,s[0].start,s[1].stop,s[0].stop))
W_P=crop(sp,so[2]); B_P=crop(sp,so[3])
for nm,f in [('lit','lit.png'),('litp','lit_pressed.png')]:
    im=white_to_alpha(Image.open(f)); a=np.asarray(im)[...,3]
    # objects incl glow: take bigger bboxes from a strong threshold then pad
    lab,n=ndimage.label(a>200)
    ss=sorted([s for s in ndimage.find_objects(lab) if s[0].stop-s[0].start>100],key=lambda s:s[1].start)
    print(nm,[(s[1].start,s[0].start,s[1].stop-s[1].start,s[0].stop-s[0].start) for s in ss])
    globals()[nm]=[(im,s) for s in ss]
# --- panel: rebuild keyboard with 16 whites
pan=Image.open('panel.png').convert('RGBA'); P=np.asarray(pan).copy()
wkey=pan.crop((53,792,135,964)); bkey=pan.crop((121,604,192,766))
# filler: per-row strips from empty tray at right side
X0,X1,Y0,Y1=44,1492,596,966
for y in range(Y0,Y1):
    src=P[y,1484:1500] if y>=780 else P[y,1442:1492]
    w=src.shape[0]
    for x in range(X0,X1): P[y,x]=src[(x-X0)%w]
base=Image.fromarray(P,'RGBA')
pitch,gap,x0=90,14,53; ww=pitch-gap
WH=[];BL=[]
for i in range(16):
    r=(x0+i*pitch,792,ww,172); WH.append(r)
    base.alpha_composite(wkey.resize((ww,172),Image.LANCZOS),(r[0],r[1]))
for after in [0,1,2,4,5,7,8,9,11,12,14]:
    cx=x0+(after+1)*pitch-gap//2; bw=66
    r=(cx-bw//2,604,bw,162); BL.append(r)
    base.alpha_composite(bkey.resize((bw,162),Image.LANCZOS),(r[0],r[1]))
save(base.convert('RGB'),'panel')
# state sprites sized to key rects
save(W_P.resize((ww,172),Image.LANCZOS),'white_pressed'); save(B_P.resize((66,162),Image.LANCZOS),'black_pressed')
def litcut(lst,idx,w,h,name):
    im,s=lst[idx]
    pad_x=int((s[1].stop-s[1].start)*0.12); pad_y=int((s[0].stop-s[0].start)*0.06)
    c=im.crop((s[1].start-pad_x,s[0].start-pad_y,s[1].stop+pad_x,s[0].stop+pad_y))
    sx=w/(s[1].stop-s[1].start); sy=h/(s[0].stop-s[0].start)
    save(c.resize((round(c.width*sx),round(c.height*sy)),Image.LANCZOS),name)
    return round(pad_x*sx),round(pad_y*sy)
print('pads',litcut(lit,0,ww,172,'white_lit'),litcut(lit,1,66,162,'black_lit'),litcut(litp,0,ww,172,'white_litp'),litcut(litp,1,66,162,'black_litp'))
# knob: tight crop
k=Image.open('knob.png'); bb=k.getbbox(); k=k.crop(bb); print('knob bbox',bb); save(k.resize((200,200),Image.LANCZOS),'knob')
print('whites',WH[:2],'blacks',BL)
