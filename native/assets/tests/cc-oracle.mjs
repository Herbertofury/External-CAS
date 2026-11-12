// SPDX-License-Identifier: GPL-3.0-or-later
// Optional real-source comparison; no asset bytes or private paths are written.
import {readFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const [configPath,root,rustReport]=process.argv.slice(2);
const config=JSON.parse(await readFile(configPath,'utf8'));
const report=JSON.parse((await readFile(rustReport,'utf8')).replace(/^\uFEFF/,''));
const {indexPackage,readResourceBatch}=await import(pathToFileURL(path.join(root,'dbpf.mjs')));
const {parseCasp}=await import(pathToFileURL(path.join(root,'casp.mjs')));
const {parseGeom}=await import(pathToFileURL(path.join(root,'geom.mjs')));
function geometryHash(g){const hash=createHash('sha256');for(const v of g.vertices){const b=Buffer.alloc(44);let at=0;for(const n of [...v.position,...v.normal,...v.uvs[0]]){b.writeFloatLE(n,at);at+=4;}for(const n of v.joints??[0,0,0,0]){b.writeUInt16LE(n,at);at+=2;}for(const n of v.weights??[0,0,0,0])b[at++]=n;hash.update(b);}for(const n of [...g.indices,...g.palette.hashes]){const b=Buffer.alloc(4);b.writeUInt32LE(n);hash.update(b);}return hash.digest('hex');}
const evidence=[];
for(const [kind,filename] of Object.entries(config.samples)){
 const index=await indexPackage(filename),observed=report.samples.find(s=>s.category===kind);
 assert.equal(index.source.sha256,observed.sourceHash);assert.equal(index.rows.length,observed.entries);
 const selected=index.rows.filter(r=>/^(034AEECB|015A1849):/.test(r.key)&&!r.deleted);
 const rows=[];for(let i=0;i<selected.length;i+=128){rows.push(...await readResourceBatch(filename,index,selected.slice(i,i+128).map(e=>e.ordinal)));}
 let casps=0,geoms=0;const geometries=[],diffuseBindings=[];
 for(const {row,payload,status} of rows){assert.equal(status,'available');const hash=createHash('sha256').update(payload).digest('hex');
  if(row.key.startsWith('034AEECB:')){const c=parseCasp(payload),r=observed.casp.find(c=>c.key===row.key);assert.ok(r,`${kind} missing CASP`);assert.equal(hash,r.hash);assert.equal(c.name,r.name);assert.equal(c.bodyType,r.bodyType);assert.equal(c.version,r.version);assert.deepEqual(c.references.map(v=>v.key),r.references);assert.deepEqual(c.roles.filter(r=>r.role==='mesh'&&r.lod===0&&!r.empty).map(r=>r.key),r.lod0);const diffuse=c.roles.find(r=>r.role==='diffuse');assert.equal(diffuse&&!diffuse.empty?diffuse.key:null,r.diffuse);casps++;
   if(diffuse&&!diffuse.empty){const [,group,instance]=diffuse.key.split(':');const candidates=index.rows.filter(r=>r.key.split(':')[1]===group&&r.key.split(':')[2]===instance);diffuseBindings.push({declared:diffuse.key,providers:candidates.map(c=>c.key)});}
  }else{const g=parseGeom(payload),r=observed.geometry.find(g=>g.key===row.key);assert.ok(r,`${kind} missing GEOM`);assert.equal(hash,r.hash);assert.equal(g.vertices.length,r.vertices);assert.equal(g.indices.length/3,r.triangles);assert.equal(g.version,r.version);assert.equal(geometryHash(g),r.decodedHash);assert.equal(g.palette.hashes.length,r.palette);geoms++;geometries.push({version:g.version,vertices:g.vertices.length,triangles:g.indices.length/3,palette:g.palette.hashes.length});}
 }
 const thumbnails=[];const thumbRows=index.rows.filter(r=>r.key.startsWith('3C1AF1F2:')&&!r.deleted).slice(0,2);
 if(thumbRows.length){for(const item of await readResourceBatch(filename,index,thumbRows.map(r=>r.ordinal))){assert.equal(item.status,'available');const instance=item.row.key.split(':')[2];thumbnails.push({key:item.row.key,bytes:item.payload.length,magic:item.payload.subarray(0,8).toString('hex').toUpperCase(),payloadHash:createHash('sha256').update(item.payload).digest('hex'),caspCandidates:observed.casp.filter(c=>c.key.split(':')[2]===instance).map(c=>c.key)});}}
 const types={};for(const r of index.rows)types[r.key.slice(0,8)]=(types[r.key.slice(0,8)]??0)+1;
 evidence.push({category:kind,sourceBytes:index.source.size,sourceHash:index.source.sha256,indexRows:index.rows.length,caspCompared:casps,geometryCompared:geoms,allDecodedGeometryBytesEqual:true,geometries,resourceTypes:types,diffuseBindings,thumbnails});
}
console.log(JSON.stringify({date:'2026-11-12',scope:'five explicit real installed CC samples; component parity only; original sources unchanged',recoveredOracle:'cas-public-1972 resource-index JavaScript readers',samples:evidence,limitations:['source selection does not establish game load order','native material, source alias semantics and complete Sim appearance remain unverified','not actual application performance or game synchronization evidence']},null,2));
