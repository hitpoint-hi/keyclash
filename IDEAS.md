# Next features for a keyboard-first Mac

Ranked by how much they help you move to the keyboard. Each has a size estimate.

1. **Fix-a-clash helper** — built (first version): a change link per row and a "Free with the same modifier keys" line. Was: (about 1 session). Each clash card gets a "How to change it" line: the exact settings screen or config file line for AeroSpace, OmniWM, Rectangle, Wispr Flow or macOS. A "Free keys near this one" list suggests open combos from the keyboard map.
2. **"My keys" cheat sheet** (about half a session). One page with only your ★ starred shortcuts, grouped by app. You can print it or keep it as a small always-on-top window while you learn them.
3. **Use counter** — built 2026-11-09 (first version): the "Use" tab and the "Use" colors on the keyboard map. Was: (about 1 session).
   - Counts each key press with ⌘, ⌃, ⌥ or 🌐, per app and per day. Typing is not recorded. The file is `~/Library/Application Support/Keyclash/usage.json`. The counter is off until the person turns it on.
   - Shows "Most used", "Starred, not pressed" and "Pressed, not in the scan".
   - Still open: double-tap and hold triggers of other tools (⌥ ⌥, hold 🌐), mouse buttons, and a count while the window is closed (item 7).
4. **Practice mode** (about 1 session). The app shows an action ("New tab in Safari") and you press the keys. It tracks which shortcuts you still look up.
5. **Hide noise** (small). Hide an app's shortcut or a whole app. The map and conflicts then show only what matters to you.
6. **Export** — built for the clash list ("Save as Markdown", `--export`). Starred keys and CSV are still open. Was: (small). Save the clash list or your starred keys as Markdown or CSV.
7. **Menu bar icon and launch at login** (small). The ⌘ ⌘ panel then always works, without the main window open.
8. **"Always on" tab: system-wide hotkeys** (about half a session). One tab with only the hotkeys that work in each app, grouped by the tool that owns them. Today this is a filter: All shortcuts › Works › "System-wide only". The tab adds the owner groups, the "not open" tag, and the clash or block mark per row.
   - Built 2026-11-09: hotkeys that apps keep as text in a JSON settings file (Todoist, Notion, Claude) are now read. Todoist ⌥Space Quick Add was absent before.
   - Still open: apps that keep hotkeys in their own database (Raycast), and app defaults that are not stored until you change them.
9. **New name** — done 2026-11-09: the app is Keyclash, with an app icon (two keycaps and a clash). Was: "Chordbook", which did not dictate well ("Cordbook"). Since 0.1.0 the app id is `app.keyclash.Keyclash` and the data folder is "Keyclash". The icon source is `assets/make_icon.swift`.
