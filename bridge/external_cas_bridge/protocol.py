"""Wire-compatible HMAC framing; no game imports and no socket auto-start."""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib,hmac,json,struct
MAGIC=b'ECASL001';DOMAIN=b'ExternalCAS local protocol v1\0';HEADER=39
class ProtocolError(ValueError):pass
def canonical(value):return json.dumps(value,ensure_ascii=False,allow_nan=False,separators=(',',':')).encode('utf-8')
class FrameCodec:
 def __init__(self,secret,nonce,outgoing,max_frame=8*1024*1024):
  if not isinstance(secret,bytes)or len(secret)!=32 or secret==bytes(32)or not isinstance(nonce,bytes)or len(nonce)!=16 or nonce==bytes(16)or outgoing not in(0,1)or not 0<max_frame<1<<32:raise ProtocolError('Pairing/session/frame bound')
  self._secret=secret;self.nonce=nonce;self.outgoing=outgoing;self.max_frame=max_frame;self.sent=0;self.received=0
 def __repr__(self):return 'FrameCodec(secret=[redacted])'
 def encode(self,value):
  payload=canonical(value)
  if len(payload)>self.max_frame or self.sent==(1<<64)-1:raise ProtocolError('Frame/sequence bound')
  sequence=self.sent+1;frame=MAGIC+struct.pack('>H',1)+self.nonce+bytes([self.outgoing])+struct.pack('>QI',sequence,len(payload))+payload
  self.sent=sequence;return frame+hmac.new(self._secret,DOMAIN+frame,hashlib.sha256).digest()
 def read(self,stream):
  def take(n):
   parts=[];left=n
   while left:
    block=stream.read(left)
    if not block:raise ProtocolError('Truncated local message')
    parts.append(block);left-=len(block)
   return b''.join(parts)
  header=take(HEADER)
  if header[:8]!=MAGIC or header[8:10]!=b'\0\1'or header[10:26]!=self.nonce or header[26]not in(0,1)or header[26]==self.outgoing:raise ProtocolError('Authentication failed')
  sequence,size=struct.unpack_from('>QI',header,27)
  if size>self.max_frame:raise ProtocolError('Frame bound')
  payload=take(size);signature=take(32)
  if not hmac.compare_digest(signature,hmac.new(self._secret,DOMAIN+header+payload,hashlib.sha256).digest()):raise ProtocolError('Authentication failed')
  if sequence!=self.received+1:raise ProtocolError('Replay or sequence gap')
  value=json.loads(payload.decode('utf-8'),parse_constant=lambda _:(_ for _ in()).throw(ProtocolError('Nonfinite JSON')))
  if not isinstance(value,dict)or not set(value)<= {'kind','body'}or 'kind'not in value:raise ProtocolError('Typed message envelope required')
  self.received=sequence;return value
