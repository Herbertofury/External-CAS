# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare morphology vertex attributes with the original Blender reader."""
import contextlib
import hashlib
import io
import json
import struct
import sys
import types

package = types.ModuleType('io_simgeom')
package.__path__ = [sys.argv[1]]
sys.modules['io_simgeom'] = package
from io_simgeom.util.globals import Globals
Globals.HASHMAP = {'bones': {}, 'shader': {}}
from io_simgeom.io.geom_load import GeomLoader
from io_simgeom.util.bytereader import ByteReader

raw = sys.stdin.buffer.read()
reader = ByteReader(raw)
reader.skip(12)
external, internal = reader.getUint32(), reader.getUint32()
if internal != 1:
    raise ValueError('unsupported chunk layout')
reader.skip((external + internal) * 16)
chunk_offset, chunk_size = reader.getUint32(), reader.getUint32()
if reader.getOffset() != chunk_offset or reader.getString(4) != 'GEOM':
    raise ValueError('invalid chunk')
version = reader.getUint32()
reader.skip(8)
shader = reader.getUint32()
if shader:
    reader.skip(reader.getUint32())
reader.skip(8)
vertex_count, format_count = reader.getUint32(), reader.getUint32()
with contextlib.redirect_stdout(io.StringIO()):
    vertices = GeomLoader.getElementData(reader, format_count, vertex_count)
digest = hashlib.sha256()
counts = {'vertexIds': 0, 'tags': 0, 'tangents': 0}
for vertex in vertices:
    uvs = vertex.uv or []
    digest.update(struct.pack('<I', len(uvs)))
    for pair in uvs:
        digest.update(struct.pack('<2f', *pair))
    tangent = vertex.tangent
    digest.update(bytes([tangent is not None]))
    if tangent is not None:
        counts['tangents'] += 1
        digest.update(struct.pack('<3f', *tangent))
    tag = vertex.tagvalue
    digest.update(bytes([tag is not None]))
    if tag is not None:
        counts['tags'] += 1
        digest.update(bytes(tag))
    vertex_id = vertex.vertex_id
    digest.update(bytes([vertex_id is not None]))
    if vertex_id is not None:
        counts['vertexIds'] += 1
        digest.update(struct.pack('<I', vertex_id[0]))
print(json.dumps({'version': version, 'vertices': len(vertices),
                  'morphAttributesHash': digest.hexdigest(), 'counts': counts}))
