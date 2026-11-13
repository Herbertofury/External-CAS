// SPDX-License-Identifier: GPL-3.0-or-later
package { import flash.display.Sprite; public class OverrideMain extends Sprite { public var namedChild:OverrideChild; public var namedBase:MovieChild; public function OverrideMain() { trace("override-root",namedChild is OverrideChild,namedBase is OverrideChild); } } }
