# Keyclash site

A static one-page site for Keyclash. It explains the app and sends people to the GitHub repository. It has no build step and makes no requests to other servers.

## Preview it

Serve the folder over http. Browsers do not run module scripts from `file://`. The keyboard demo works from a file, but the motion and the phone menu do not.

```sh
cd site
python3 -m http.server 8000
# open http://127.0.0.1:8000
```

## What is in the folder

| Path | Content |
|---|---|
| `index.html` | The page, its page CSS, and the script for the keyboard map demo |
| `assets/` | The layout, motion and UI kit from the `design-taste-frontend` skill (system.css, system.js, motion, ui, layout, the iris-opens moment) |
| `fonts/` | Schibsted Grotesk and JetBrains Mono (latin subset), with their SIL Open Font License files |
| `icon-512.png`, `logo-192.png` | Copies of the app icon from `../assets/` |
| `share.jpg` | The link preview picture: a still of the hero |

`.taste-skill/` and `shots/` hold design-check output. `.gitignore` keeps them out of git.

## The page

1. Hero: "A keyboard shortcut map for your Mac", with the Conflicts tab drawn in HTML.
2. Keyboard map: an interactive copy of the Keyboard map tab. Click or hold ⌃ ⌥ ⇧ ⌘, then click a key.
3. Peek, four feature cards, privacy (the data folder), build from source, and questions.
4. The close and footer point to GitHub.

All windows use demo data with Apple apps and tools that Keyclash reads (AeroSpace, skhd, Rectangle, Wispr Flow). No screenshots from `shots/` are used, because they show the old name and a real app list.

## Before it goes live

1. **The repo is private.** Every GitHub link is a 404 for visitors until `hitpoint-hi/keyclash` is public.
2. **Licence: done.** MIT, in `../LICENSE`. The footer links to it.
3. **Fresh history: prepared** on the local `public` branch, without the internal review records.
4. **Vendored kit code.** `assets/` comes from the `design-taste-frontend` skill. Its licence for redistribution is not confirmed.
5. **Domain.** Turn on the `canonical` link in the head, and make `og:image` an absolute URL, when keyclash.app points here.
6. **Hosting.** Any static file server works. The page has no inline scripts, so a strict `script-src 'self'` Content-Security-Policy works.

## Design checks

- Layout check (`check.mjs`): no findings at 1440, 1024, 820, 768 and 390.
- Copy lint: 0 FAIL, 0 WARN. Speed (`perf.mjs`): 0 FAIL.
- The finish gate ended `OPEN` after its three rounds. The last full `consistency.mjs` run had 12 FAILs:
  - 6 `button-styles`: the keycaps of the keyboard demo count as button styles. Waived in `.taste-skill/waivers.json`, but no gate run read the waiver.
  - Tablet only (768 and 820): empty space in the features section and gaps beside it.
  - A wide gap under the questions at 1440.
  - `scroll-reveal` on the closing button: the kit's motion layer, not page code.
  - At 320px wide only: the last answer runs into the footer.
- The Impeccable hook flagged the web fonts as outside `DESIGN.md`. `DESIGN.md` records the native app (SF Pro). Three ignore values for the web fonts went into `../.impeccable/config.json`.
