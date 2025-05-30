"""Render embedded Windows icon resources without starting the application or a window."""
import argparse
import ctypes as c
import json
import struct
from ctypes import wintypes as w
from pathlib import Path
from PIL import Image, ImageDraw


def inspect(executable: Path, source: Path, output: Path):
    raw = source.read_bytes()
    reserved, kind, count = struct.unpack_from('<HHH', raw)
    assert (reserved, kind) == (0, 1)
    sizes, bounds = [], {}
    source_image = Image.open(source)
    for frame in source_image.ico.sizes():
        image = source_image.ico.getimage(frame).convert('RGBA')
        box = image.getchannel('A').getbbox()
        assert box and image.getchannel('A').getextrema()[1] == 255, f'Blank icon {frame}'
        assert image.getpixel((0, 0))[3] <= 8, f'Clipped corner {frame}'
        sizes.append(frame[0]); bounds[str(frame[0])] = box
    assert sorted(sizes) == [16,20,24,32,40,48,64,128,256]
    kernel, user, gdi = c.WinDLL('kernel32', use_last_error=True), c.WinDLL('user32', use_last_error=True), c.WinDLL('gdi32', use_last_error=True)
    kernel.LoadLibraryExW.argtypes=[w.LPCWSTR,w.HANDLE,w.DWORD]; kernel.LoadLibraryExW.restype=w.HMODULE
    kernel.FreeLibrary.argtypes=[w.HMODULE]
    user.LoadImageW.argtypes=[w.HINSTANCE,w.LPCWSTR,w.UINT,c.c_int,c.c_int,w.UINT];user.LoadImageW.restype=w.HANDLE
    user.DestroyIcon.argtypes=[w.HANDLE]
    user.DrawIconEx.argtypes=[w.HDC,c.c_int,c.c_int,w.HANDLE,c.c_int,c.c_int,w.UINT,w.HBRUSH,w.UINT]
    gdi.CreateCompatibleDC.argtypes=[w.HDC];gdi.CreateCompatibleDC.restype=w.HDC
    gdi.CreateDIBSection.argtypes=[w.HDC,c.c_void_p,w.UINT,c.POINTER(c.c_void_p),w.HANDLE,w.DWORD];gdi.CreateDIBSection.restype=w.HBITMAP
    gdi.SelectObject.argtypes=[w.HDC,w.HANDLE];gdi.SelectObject.restype=w.HANDLE
    gdi.DeleteObject.argtypes=[w.HANDLE];gdi.DeleteDC.argtypes=[w.HDC]
    module=kernel.LoadLibraryExW(str(executable.resolve()),None,0x2|0x20)
    if not module: raise c.WinError(c.get_last_error())
    sheet=Image.new('RGB',(520,340),'#eff2f5');draw=ImageDraw.Draw(sheet)
    draw.text((12,8),'Embedded resource proof - native DrawIconEx, no application window',fill='#18202a')
    try:
        for row,size in enumerate([16,20,24,32,48,64]):
            icon=user.LoadImageW(module,c.cast(c.c_void_p(1),w.LPCWSTR),1,size,size,0)
            if not icon: raise c.WinError(c.get_last_error())
            try:
                for col,bg in enumerate([(244,246,248),(28,32,39)]):
                    width,height=220,40
                    if size>32: width,height=220,70
                    dc=gdi.CreateCompatibleDC(None)
                    info=struct.pack('<IiiHHIIiiII',40,width,-height,1,32,0,width*height*4,0,0,0,0)
                    bits=c.c_void_p()
                    bmp=gdi.CreateDIBSection(dc,info,0,c.byref(bits),None,0)
                    if not bmp: raise c.WinError(c.get_last_error())
                    old=gdi.SelectObject(dc,bmp)
                    try:
                        c.memmove(bits,bytes((bg[2],bg[1],bg[0],255))*width*height,width*height*4)
                        if not user.DrawIconEx(dc,8,(height-size)//2,icon,size,size,0,None,3): raise c.WinError(c.get_last_error())
                        image=Image.frombytes('RGBA',(width,height),c.string_at(bits,width*height*4),'raw','BGRA').convert('RGB')
                        diff=image.crop((8,(height-size)//2,8+size,(height+size)//2))
                        assert diff.getextrema() != tuple((value, value) for value in bg), f'Blank embedded image {size}'
                        y=28+row*43 if row<4 else 200+(row-4)*70
                        sheet.paste(image,(12+col*248,y));draw.text((90+col*248,y+12),f'{size} px',fill='#18202a' if col==0 else '#f4f6f8')
                    finally:
                        gdi.SelectObject(dc,old);gdi.DeleteObject(bmp);gdi.DeleteDC(dc)
            finally: user.DestroyIcon(icon)
    finally: kernel.FreeLibrary(module)
    output.parent.mkdir(parents=True,exist_ok=True);sheet.save(output)
    return {'executable':str(executable.resolve()),'source_sizes':sorted(sizes),'source_alpha_bounds':bounds,'proof':str(output.resolve()),'scope':'Native embedded resource load and drawing; no shell tray or taskbar interaction.'}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('executable',type=Path);parser.add_argument('source',type=Path);parser.add_argument('output',type=Path)
    args=parser.parse_args();print(json.dumps(inspect(args.executable,args.source,args.output),indent=2))
