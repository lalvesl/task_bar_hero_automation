# UI map

Every coordinate here is normalized against the game window, so it survives the
window moving and the virtual screen being resized. Multiply by the live window
rectangle at click time; never store absolute screen pixels.

Captured on 2026-09-11 against a 1212x1115 window on an 1800x1200 virtual
screen, game version 1.0.4.

## Why these stay valid

The game's panels open side by side at fixed positions and do not push each
other around. Opening the stash does not move the cube. That is the property
that makes a table of fixed points workable at all, and it is why v1 needs
template matching only for dropped chests, whose position is genuinely
unpredictable.

The window size itself is pinned by the player's own Unity setting, stored in
the Wine registry under `Software\TesseractStudio\TaskBarHero`.

## Keys

Prefer a key over a click wherever the game offers one: a key carries no
position, so it cannot be invalidated by a layout change.

| Action | Key |
| --- | --- |
| Open the main menu | Tab |

## Main menu, bottom row

| Element | x | y |
| --- | --- | --- |
| Stash | 0.382 | 0.723 |
| Cross | 0.441 | 0.723 |
| Hourglass | 0.500 | 0.723 |
| Cube | 0.559 | 0.723 |
| Gem | 0.617 | 0.723 |

## Stash panel

| Element | x | y |
| --- | --- | --- |
| Tab 1 | 0.025 | 0.343 |
| New tab | 0.079 | 0.343 |
| Take all | 0.080 | 0.728 |
| Store all | 0.212 | 0.728 |
| Side menu | 0.300 | 0.728 |
| Close | 0.303 | 0.298 |

## Cube panel

| Element | x | y |
| --- | --- | --- |
| Mode selector (Synthesis) | 0.774 | 0.346 |
| Level range selector | 0.916 | 0.346 |
| Auto-fill | 0.765 | 0.595 |
| Auto-fill options | 0.837 | 0.595 |
| Synthesize | 0.906 | 0.595 |
| Include stash items | 0.758 | 0.637 |
| Close | 0.966 | 0.298 |

The cube does carry selectors, answering the question M3 was meant to settle:
one for the mode and one for the item level range. Auto-fill still chooses the
items, so no task ever has to read an item's rarity off the screen.

## The synthesize button's two states

This is the only thing the cube task has to recognise, and it is a few pixels
rather than a template.

Sampled at these window pixels on the reference capture:

| Window pixel | Disabled | Enabled |
| --- | --- | --- |
| 1060, 650 | 113, 113, 113 | 33, 81, 115 |
| 1120, 678 | 73, 73, 73 | 0, 81, 115 |
| 1135, 664 | 51, 51, 51 | 0, 48, 63 |

Note the shape of it. Disabled is pure grey: the three channels are equal.
Enabled is blue: the blue channel leads red by about 80.

So the check is not "does this pixel match this colour within a tolerance" but
"does blue lead red by a margin". That is immune to brightness drift, to hover
highlighting, and to any repaint a patch might bring, in a way an absolute
colour comparison is not.

## Task order

Store all before any synthesis run. Auto-fill draws from the stash as well as
the inventory when "include stash items" is ticked, so depositing first is what
makes the full inventory available to the cube.
