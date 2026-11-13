// SPDX-License-Identifier: GPL-3.0-or-later
package { import flash.display.Sprite; import flash.external.ExternalInterface; public class NativeProjectionFixture extends Sprite {
 public static var nativeSlot:Function; public var hash:uint=35; public var tokens:Array=null; public var flags:uint=0;
 public function NativeProjectionFixture(){ExternalInterface.addCallback("InvokeNativeProjection",invokeNativeProjection);}
 public function invokeNativeProjection():Object{return nativeSlot(this);}
} }
