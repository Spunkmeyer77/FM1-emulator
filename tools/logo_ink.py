# Silk-screen look for the TR logo and model name (run after build_skin.py).
from PIL import Image, ImageFilter
import numpy as np
F='/home/claude/fm1-emulator/crates/fm1-plugin/skin/panel.png'
p=Image.open(F).convert('RGBA'); box=(1140,4,1510,84)
reg=p.crop(box); a=np.asarray(reg).astype(float); lum=a[...,:3].mean(2)
ink=np.clip((150-lum)/70,0,1)
bg=np.asarray(reg.filter(ImageFilter.MaxFilter(9))).astype(float)
plastic=a*(1-ink[...,None])+bg*ink[...,None]
rng=np.random.default_rng(1)
inkm=np.asarray(Image.fromarray((ink*255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.6))).astype(float)/255
g=rng.normal(0,1,lum.shape)
g=np.asarray(Image.fromarray((g*30+128).clip(0,255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.6))).astype(float)-128
inkc=np.dstack([58+g*0.45,37+g*0.35,24+g*0.25])
cov=(inkm*(1.0+g/500))[...,None].clip(0,1)
out=plastic[...,:3]*(1-cov)+inkc*cov
out+=(np.roll(inkm,-1,0)-inkm).clip(0,1)[...,None]*16
out-=(np.roll(inkm,1,0)-inkm).clip(0,1)[...,None]*10
p.paste(Image.fromarray(np.dstack([out.clip(0,255),a[...,3]]).astype(np.uint8),'RGBA'),box[:2])
p.save(F,optimize=True)
