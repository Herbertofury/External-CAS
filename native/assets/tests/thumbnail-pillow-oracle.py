"""Read-only independent Pillow comparison; framed pixels only over stdio."""
# SPDX-License-Identifier: GPL-3.0-or-later
import io,struct,sys,warnings
sys.path.insert(0,sys.argv[1])
from PIL import Image,ImageFile
Image.MAX_IMAGE_PIXELS=16*1024*1024
ImageFile.LOAD_TRUNCATED_IMAGES=False
def take(n):
    b=sys.stdin.buffer.read(n)
    if len(b)!=n: raise ValueError('truncated oracle frame')
    return b
def word():return struct.unpack('<I',take(4))[0]
count=word()
if count>4096:raise ValueError('oracle count')
out=sys.stdout.buffer;out.write(struct.pack('<I',count))
for _ in range(count):
    size=word()
    if size>64*1024*1024:raise ValueError('oracle payload')
    raw=take(size)
    with warnings.catch_warnings():
        warnings.simplefilter('error',Image.DecompressionBombWarning)
        # Decode the original writer container directly. Pillow/libjpeg can
        # parse its extraJFIFtail; the Rust path independently reconstructs it.
        with Image.open(io.BytesIO(raw))as source:
            source.load();rgba=source.convert('RGBA')
        if raw[:2]==b'\xff\xd8'and raw[24:28]==b'ALFA':
            n=struct.unpack_from('>I',raw,28)[0]
            with Image.open(io.BytesIO(raw[32:32+n]))as source:
                if source.size!=rgba.size or source.format!='PNG':raise ValueError('alpha dimensions/format')
                source.load();rgba.putalpha(source.convert('RGBA').getchannel('R'))
        pixels=rgba.tobytes();out.write(struct.pack('<III',rgba.width,rgba.height,len(pixels)));out.write(pixels)
out.flush()
