// SPDX-License-Identifier: GPL-3.0-or-later
package { import flash.display.Sprite; import flash.external.ExternalInterface; public class NativeSlotFixture extends Sprite { public function NativeSlotFixture() { ExternalInterface.addCallback("InvokeNative",invokeNative); } public function invokeNative(input:Object):Object { return NativeBindingTarget.nativeSlot(input); } } }
