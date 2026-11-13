// SPDX-License-Identifier: GPL-3.0-or-later
package { import flash.display.MovieClip; public class MovieChild extends MovieClip { public function MovieChild() { trace("class-only-child",name,parent!=null); } } }
