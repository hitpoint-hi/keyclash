# Keyclash website

The static homepage at https://keyclash.app introduces the Mac download, an interactive shortcut map, and a 65-second product tour.

## Local preview

From this folder, run:

```sh
python3 -m http.server 8000 --bind 127.0.0.1
```

Open http://127.0.0.1:8000. The site has no build step. The page loads its fonts, artwork, tour, and keyboard demo from this folder.

## Page files

- `index.html` contains the page, download links, transcript, and installation guide.
- `homepage.css` defines the layout and keyboard states.
- `keymap.js` runs the example keyboard. Its sample data is independent of the visitor’s Mac.
- `media/` contains the keycap artwork, video, and video poster.
- `fonts/` contains the local fonts and their SIL Open Font Licenses.
- `share.jpg` supplies the social link preview.

## Publish

Run `scripts/deploy-site.sh` from the repository root to publish the committed site. Use `--dry-run` to compare against the live release or `--rollback` to restore the previous release.
