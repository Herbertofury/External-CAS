// SPDX-License-Identifier: GPL-3.0-or-later
// Read-only comparison against recovered independently tested parser sources.
import {readFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
import {createHash} from 'node:crypto';
const [manifest,root]=process.argv.slice(2),j=JSON.parse(await readFile(manifest));
const {indexPackage,readResource}=await import(pathToFileURL(path.join(root,'dbpf.mjs')));
const {parseCasp}=await import(pathToFileURL(path.join(root,'casp.mjs')));
const {parseGeom}=await import(pathToFileURL(path.join(root,'geom.mjs')));
function indexHash(index){const hash=createHash('sha256');for(const e of index.rows){const [ty,gr,inst]=e.key.split(':'),b=Buffer.alloc(41);let at=0;const u32=n=>{b.writeUInt32LE(n,at);at+=4;};u32(parseInt(ty,16));u32(parseInt(gr,16));b.writeBigUInt64LE(BigInt('0x'+inst),at);at+=8;u32(e.ordinal);b.writeBigUInt64LE(BigInt(e.offset),at);at+=8;u32(e.storedSize);u32(e.memorySize);b.writeUInt16LE(e.compression,at);at+=2;b.writeUInt16LE(e.committed,at);at+=2;b[at]=Number(e.extended);hash.update(b);}return hash.digest('hex');}
function geometryHash(g){const hash=createHash('sha256');for(const v of g.vertices){const b=Buffer.alloc(44);let at=0;for(const n of [...v.position,...v.normal,...v.uvs[0]]){b.writeFloatLE(n,at);at+=4;}for(const n of v.joints??[0,0,0,0]){b.writeUInt16LE(n,at);at+=2;}for(const n of v.weights??[0,0,0,0])b[at++]=n;hash.update(b);}for(const n of [...g.indices,...g.palette.hashes]){const b=Buffer.alloc(4);b.writeUInt32LE(n);hash.update(b);}return hash.digest('hex');}
const indexes=await Promise.all(j.sources.map(s=>indexPackage(s.path)));
const c=parseCasp((await readResource(j.sources[0].path,indexes[0],j.casp.ordinal)).payload);
const geoms=[];for(const r of j.selectedResources){if(!r.key.startsWith('015A1849:'))continue;const g=parseGeom((await readResource(j.sources[r.source].path,indexes[r.source],r.ordinal)).payload);geoms.push({key:r.key,hash:geometryHash(g),vertices:g.vertices.length,indices:g.indices.length,palette:g.palette.hashes});}
console.log(JSON.stringify({indexes:indexes.map(i=>({count:i.rows.length,hash:indexHash(i)})),casp:{name:c.name,version:c.version,bodyType:c.bodyType,diffuse:c.roles.find(r=>r.role==='diffuse').key,lod0:c.roles.filter(r=>r.role==='mesh'&&r.lod===0).map(r=>r.key)},geoms}));
