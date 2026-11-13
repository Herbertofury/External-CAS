// SPDX-License-Identifier: GPL-3.0-or-later
package { import flash.display.Sprite; import flash.external.ExternalInterface; public class HostCallbackFixture extends Sprite { public function HostCallbackFixture() { ExternalInterface.call("RetainCallback",function(value:Number):Number { return value+7; }); } } }
