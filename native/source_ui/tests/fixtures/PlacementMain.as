// SPDX-License-Identifier: GPL-3.0-or-later
package { import flash.display.Sprite; public class PlacementMain extends Sprite { public var namedChild:MovieChild; public function PlacementMain() { trace("class-only-root",namedChild!=null,namedChild?namedChild.name:"missing"); } } }
