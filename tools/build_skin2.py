# Skin assets for the large-screen panel (FM-1_Panel_Large_Screen.png).
from PIL import Image, ImageDraw, ImageFilter
import numpy as np
from scipy import ndimage
OUT='/home/claude/fm1-emulator/crates/fm1-plugin/skin/'
def save(img,name): img.save(OUT+name+'.png',optimize=True)
def white_to_alpha(img):
    a=np.asarray(img.convert('RGB')).astype(float)
    al=np.clip((255-a.min(2))/255,0,1); al=np.where(al<0.03,0,al)
    col=np.where(al[...,None]>0,(a-255*(1-al[...,None]))/np.maximum(al[...,None],1e-3),0)
    return Image.fromarray(np.dstack([np.clip(col,0,255),al*255]).astype(np.uint8),'RGBA')
WW,WH,BW,BH=76,129,66,123
# pressed sprites
sp=Image.open('sprites.png'); lab,n=ndimage.label(np.asarray(sp)[...,3]>20)
so=sorted([s for s in ndimage.find_objects(lab) if s[0].stop-s[0].start>40],key=lambda s:(s[0].start//100,s[1].start))
crop=lambda im,s: im.crop((s[1].start,s[0].start,s[1].stop,s[0].stop))
save(crop(sp,so[2]).resize((WW,WH),Image.LANCZOS),'white_pressed'); save(crop(sp,so[3]).resize((BW,BH),Image.LANCZOS),'black_pressed')
pads={}
for nm,f in [('lit','lit.png'),('litp','lit_pressed.png')]:
    im=white_to_alpha(Image.open(f)); a=np.asarray(im)[...,3]
    lab,n=ndimage.label(a>200)
    ss=sorted([s for s in ndimage.find_objects(lab) if s[0].stop-s[0].start>100],key=lambda s:s[1].start)
    for idx,(w,h,kind) in enumerate([(WW,WH,'white'),(BW,BH,'black')]):
        s=ss[idx]; sw=s[1].stop-s[1].start; sh=s[0].stop-s[0].start
        px,py=int(sw*0.12),int(sh*0.06)
        c=im.crop((s[1].start-px,s[0].start-py,s[1].stop+px,s[0].stop+py))
        sx,sy=w/sw,h/sh
        save(c.resize((round(c.width*sx),round(c.height*sy)),Image.LANCZOS),f'{kind}_{nm}')
        pads[f'{kind}_{nm}']=(round(px*sx),round(py*sy))
print('pads',pads)
# panel: rounded corners
base=Image.open('panel3.png').convert('RGBA')
R=26; ss=4
m=Image.new('L',(base.width*ss,base.height*ss),0)
ImageDraw.Draw(m).rounded_rectangle((0,0,m.width-1,m.height-1),R*ss,fill=255)
base.putalpha(m.resize(base.size,Image.LANCZOS))
save(base,'panel')
k=Image.open('knob.png'); k=k.crop(k.getbbox()); save(k.resize((200,200),Image.LANCZOS),'knob')
