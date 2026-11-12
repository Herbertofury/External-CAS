// SPDX-License-Identifier: GPL-3.0-or-later
// Read-only, private in-memory comparison against retained source-tool curves.
// Requires explicit local inputs. No Studio/game launches or asset writes.
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import path from 'node:path';

const required=['CAS_ANIMATION_REFERENCE_ROOT','CAS_ANIMATION_RIG','CAS_ANIMATION_CLIP_PACKAGE','CAS_ANIMATION_BRIDGE'];
for(const name of required) if(!path.isAbsolute(process.env[name]??'')) throw Error(`Missing absolute ${name}`);
const root=process.env.CAS_ANIMATION_REFERENCE_ROOT;
const load=name=>import(pathToFileURL(path.join(root,name)));
const [{indexPackage,readResourceBatch},{parseRig,computeBindMatrices},{inspectPinnedClip},{decodePinnedClipValues},{preparePinnedClipChannel}]=await Promise.all([
  load('dbpf.mjs'),load('rig.mjs'),load('clip-metadata.mjs'),load('clip-values.mjs'),load('clip-sample.mjs'),
]);
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const rigBytes=await readFile(process.env.CAS_ANIMATION_RIG);
if(process.env.CAS_ANIMATION_RIG_SHA256 && sha(rigBytes)!==process.env.CAS_ANIMATION_RIG_SHA256) throw Error('Rig pin mismatch');
const rig=parseRig(rigBytes),bind=computeBindMatrices(rig,{scaleTolerance:1e-6});
if(!bind.local) throw Error('Reference rig unsupported');
const packagePath=process.env.CAS_ANIMATION_CLIP_PACKAGE,index=await indexPackage(packagePath);
if(process.env.CAS_ANIMATION_PACKAGE_SHA256 && index.source.sha256!==process.env.CAS_ANIMATION_PACKAGE_SHA256) throw Error('Package pin mismatch');
const rows=index.rows.filter(row=>row.key.startsWith('6B20C4F3:')&&!row.deleted&&row.committed===1&&row.memorySize<=128*1024);
const row=process.env.CAS_ANIMATION_CLIP_KEY ? rows.find(row=>row.key===process.env.CAS_ANIMATION_CLIP_KEY) : rows[0];
if(!row) throw Error('Requested clip missing');
const [record]=await readResourceBatch(packagePath,index,[row.ordinal]);
if(record.status!=='available') throw Error('Clip unavailable');
const clip=record.payload,clipSha256=sha(clip),meta=inspectPinnedClip(clip,clipSha256),decoded=decodePinnedClipValues(clip,clipSha256);
// This check targets the prior real-app pose case, not an arbitrary clip.
if(meta.codec?.ticks!==31||meta.events.length!==3||meta.slots.length!==6) throw Error('Historical clip markers differ');
const ticks=[0,5,10,15,20,30];
const native=JSON.parse(execFileSync(process.env.CAS_ANIMATION_BRIDGE,[],{
  input:JSON.stringify({rig:[...rigBytes],clip:[...clip],ticks,skipNonBone:true}),encoding:'utf8',maxBuffer:16*1024*1024,windowsHide:true,
}));
if(native.samples.some(sample=>sample.error)) throw Error('Native pose sampling failed');
if(native.rig.bones.length!==rig.bones.length||native.rig.chains!==rig.chains.length) throw Error('Rig extent mismatch');
let comparedRigFields=0,maxBindDifference=0;
for(let bone=0;bone<rig.bones.length;bone++) {
  const actual=native.rig.bones[bone],expected=rig.bones[bone];
  for(const key of ['name','hash','parentIndex']) {if(actual[key]!==expected[key]) throw Error('Rig identity mismatch');comparedRigFields++;}
  for(const key of ['position','orientation','scale']) for(let k=0;k<actual[key].length;k++) {if(actual[key][k]!==expected[key][k]) throw Error('Rig transform mismatch');comparedRigFields++;}
  for(let k=0;k<16;k++) maxBindDifference=Math.max(maxBindDifference,Math.abs(native.rig.bindRows[bone][k]-bind.global[bone][k]));
}
let decodedFrames=0,decodedComponents=0,maxValueDifference=0;
for(let channel=0;channel<decoded.channels.length;channel++) {
  const actual=native.clip.channels[channel],expected=decoded.channels[channel];
  if(actual.target!==expected.target||actual.type!==expected.type||actual.subTarget!==expected.subTarget) throw Error('Channel identity mismatch');
  if(expected.status!=='decoded-value-candidate') continue;
  if(actual.support!=='Decoded'||actual.frames.length!==expected.frames.length) throw Error('Channel support mismatch');
  for(let frame=0;frame<expected.frames.length;frame++) {
    const a=actual.frames[frame],b=expected.frames[frame];
    if(a.tick!==b.tick||a.flags!==b.flags) throw Error('Frame identity mismatch');
    decodedFrames++;
    for(let k=0;k<b.values.length;k++) {maxValueDifference=Math.max(maxValueDifference,Math.abs(a.values[k]-Math.fround(b.values[k])));decodedComponents++;}
  }
}
const multiply=(a,b)=>Array.from({length:16},(_,index)=>[0,1,2,3].reduce((sum,k)=>sum+a[(index>>2)*4+k]*b[k*4+index%4],0));
const prepared=[];let scaleTracks=0,curves=0;
for(let channelIndex=0;channelIndex<meta.codec.channels.length;channelIndex++) {
  const channel=meta.codec.channels[channelIndex];
  if(!channel.frameCount||channel.subTarget>=4) continue;
  const bone=rig.bones.findIndex(bone=>bone.hash===channel.target);
  if(bone<0) throw Error('Active bone track absent from rig');
  if(channel.subTarget===3) {
    if(decoded.channels[channelIndex].status!=='decoded-value-candidate') throw Error('Scale decoder unavailable');
    prepared.push({bone,component:3,channel:decoded.channels[channelIndex]});scaleTracks++;
  } else {
    const curve=preparePinnedClipChannel({clipBytes:clip,clipSha256,channelIndex,target:channel.target,type:channel.type,subTarget:channel.subTarget,policy:'s4studio-importer-constant-components',ancillaryPolicy:'studio-ignore-events-and-ik'});
    if(curve.status!=='prepared-source-tool-curve') throw Error('Reference bone curve unavailable');
    prepared.push({bone,component:channel.subTarget,curve});curves++;
  }
}
let comparedPaletteComponents=0,maxPaletteDifference=0,changedComponents=0;
for(let sampleIndex=0;sampleIndex<ticks.length;sampleIndex++) {
  const tick=ticks[sampleIndex],locals=bind.local.map(matrix=>matrix.slice()),scales=rig.bones.map(bone=>bone.scale.slice());
  for(const track of prepared) {
    let values;
    if(track.curve) {const sampled=track.curve.sample(tick);if(sampled.status!=='source-tool-curve-sample') throw Error('Reference sampling failed');values=sampled.values;}
    else {let frame=track.channel.frames[0];for(const candidate of track.channel.frames) {if(candidate.tick>tick) break;frame=candidate;}values=frame.values.map(Math.fround);}
    const local=locals[track.bone];
    if(track.component===1) {local[3]=values[0];local[7]=values[1];local[11]=values[2];}
    else if(track.component===3) {scales[track.bone]=values;}
    else {
      let [x,y,z,w]=values,norm=Math.hypot(...values);x/=norm;y/=norm;z/=norm;w/=norm;
      const rotation=[1-2*y*y-2*z*z,2*x*y-2*z*w,2*x*z+2*y*w,2*x*y+2*z*w,1-2*x*x-2*z*z,2*y*z-2*x*w,2*x*z-2*y*w,2*y*z+2*x*w,1-2*x*x-2*y*y];
      for(let row=0;row<3;row++) for(let column=0;column<3;column++) local[row*4+column]=rotation[row*3+column];
    }
  }
  for(let bone=0;bone<locals.length;bone++) for(let column=0;column<3;column++) {
    const norm=Math.hypot(locals[bone][column],locals[bone][4+column],locals[bone][8+column]);
    for(let row=0;row<3;row++) locals[bone][row*4+column]=locals[bone][row*4+column]/norm*scales[bone][column];
  }
  const globals=[];
  for(let bone=0;bone<rig.bones.length;bone++) {const parent=rig.bones[bone].parentIndex;globals.push(parent<0?locals[bone]:multiply(globals[parent],locals[bone]));}
  for(let bone=0;bone<rig.bones.length;bone++) {
    const reference=multiply(globals[bone],bind.inverse[bone]);
    for(let row=0;row<3;row++) for(let column=0;column<4;column++) {
      const actual=native.samples[sampleIndex].palette[bone][column*4+row],expected=Math.fround(reference[row*4+column]);
      maxPaletteDifference=Math.max(maxPaletteDifference,Math.abs(actual-expected));comparedPaletteComponents++;
      if(sampleIndex>0&&Math.abs(actual-native.samples[0].palette[bone][column*4+row])>1e-5) changedComponents++;
    }
  }
}
if(maxValueDifference>1e-6||maxBindDifference>1e-4||maxPaletteDifference>2e-4||!changedComponents) throw Error('Native/reference numerical mismatch');
console.log(JSON.stringify({rigBones:rig.bones.length,comparedRigFields,decodedFrames,decodedComponents,maxValueDifference,maxBindDifference,samples:ticks.length,ticks,sourceToolCurves:curves,scaleTracks,skippedControlTracks:native.samples[0].skippedChannels.length,comparedPaletteComponents,maxPaletteDifference,changedComponents,reference:'retained JS decoders and source curves with TRS hierarchy/inverse bind',eventsPreservedIgnored:meta.events.length,ikSlotsPreservedIgnored:meta.slots.length,nativeGameFidelity:'unverified',assetOutputs:'memory only; no Studio/game launch'}));
