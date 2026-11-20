---
name: Keyclash
description: Compact native macOS shortcut inventory and conflict reference.
colors:
  dark-background: "#0F1219"
  dark-surface: "#151821"
  dark-card: "#171B25"
  dark-raised: "#1F2431"
  dark-text: "#EEF0F4"
  dark-secondary: "#C3C8D2"
  dark-muted: "#8E95A3"
  dark-accent: "#5CA3FA"
  dark-clash: "#FF5C61"
  dark-blocked: "#FF9A2E"
  light-background: "#F4F6FA"
  light-surface: "#FFFFFF"
  light-card: "#FFFFFF"
  light-raised: "#EAEDF3"
  light-text: "#151821"
  light-secondary: "#464D5E"
  light-muted: "#5F6877"
  light-accent: "#1F63D0"
  light-clash: "#C21F2B"
  light-blocked: "#AD4B00"
typography:
  title:
    fontFamily: "SF Pro"
    fontSize: "18px"
  heading:
    fontFamily: "SF Pro"
    fontSize: "20px"
  body:
    fontFamily: "SF Pro"
    fontSize: "13.5px"
  label:
    fontFamily: "SF Pro"
    fontSize: "12.5px"
rounded:
  button: "7px"
  segmented: "8px"
  card: "12px"
  chip: "13px"
spacing:
  horizontal: "8px"
  vertical: "6px"
components:
  card-dark:
    backgroundColor: "{colors.dark-card}"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.card}"
    padding: "12px 16px"
  card-light:
    backgroundColor: "{colors.light-card}"
    textColor: "{colors.light-text}"
    rounded: "{rounded.card}"
    padding: "12px 16px"
  button-dark:
    backgroundColor: "{colors.dark-raised}"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.button}"
    padding: "5px 11px"
---

# Design System: Keyclash

## Overview

**Creative North Star: "Compact shortcut reference"**

Preserve the existing compact interface that Bobby confirmed on 2026-11-12.
The interface supports repeated lookup and comparison. App names, actions, and key combinations carry the hierarchy.
This record describes the current Rust/egui implementation. It does not establish a new visual direction.

**Key Characteristics:**
- Dense shortcut rows grouped by app or conflict.
- Native macOS interface fonts and recognizable modifier symbols.
- Light, dark, and system appearance choices.
- Explicit origin, uncertainty, and running-state labels.

## Colors

`src/theme.rs` supplies both appearances. Use the active theme through `th()`.
The frontmatter records a subset of those exact source values. Preserve the other modifier and status colors in the source.
Blue identifies selection and focus. Red identifies clashes. Orange identifies global blockers.
On 2026-11-19 Bobby asked for the light theme to use the keyclash.app palette. That palette comes from the app icon
(`assets/make_icon.swift`): paper background, navy ink, the blue key for the accent, and the red key for clashes.
The accent is darker than the icon's blue key so that text keeps 4.5:1 contrast. The clash red already matched the icon's red key at a darker shade, so it stays.
On the same day the dark theme moved to the site's dark sections: navy surfaces from the icon tile, a blue between the icon's two blue-key stops,
spark yellow for Option, warnings and stars, and spark orange for global blockers. The clash red #FF5C61 stays.
It is a shade within the icon's red-key gradient, and the darker icon red fails 4.5:1 on its badge tint.
Neutral surfaces distinguish the window, header, cards, and controls.

**The Evidence Rule.** A status color accompanies a text label. Color alone does not establish a conflict or action outcome.

## Typography

The app loads the macOS interface font from `SFNS.ttf`, with symbol fallbacks.
Code and paths use `SFNSMono.ttf`. UI sizes are egui logical points, represented as pixels in portable tokens.
The header title uses the title role. Section headings use the heading role. Dense rows use the body and label roles.
Emphasis uses the existing `medium` and `semi` helpers.

## Layout

The main window starts at 1240 by 820 logical points, with a minimum of 900 by 560.
A shared header provides navigation, scan status, Settings access, and counts.
Content uses vertical scroll areas. Root margins are 20 horizontally and 14 vertically.
Cards use 16 horizontal and 12 vertical points of padding.
The keyboard map follows the active character layout on a US ANSI drawing. Alternate physical shapes remain outside current support.

## Elevation & Depth

Tonal surfaces and thin borders separate content. Keycaps add a small downward edge.
The main hierarchy uses surface color rather than decorative shadows.

## Shapes

Cards have rounded corners. Filter chips are compact pills. Standard controls use smaller corner radii.
Retain the source geometry when adding accessibility information.

## Components

### Buttons and inputs

Use standard egui controls where possible. Preserve readable action labels and egui's keyboard behavior.
Inputs use the active field color and visible focus styling.

### Navigation

Tabs use text and a blue underline for the active view. Expose each tab's name, role, and selected state.
Keyboard focus adds an outline independent of the selected underline.

### Settings

Settings opens in one separate, nonmodal native window from the header or with ⌘,.
It preserves the selected main tab. Closing Settings leaves the main window open.
The window starts at 680 by 760 logical points and supports resizing down to 560 by 420.
Its cards fill the window width. A vertical scroll area keeps all controls accessible in shorter windows.
Appearance, Peek, use counting, local data, and About share the existing theme and controls.
⌘W, Escape, and the native close button close Settings. Changes apply immediately.

### Peek placement

Restore Peek within the visible area of a connected display, clear of the menu bar and Dock.
Preserve saved positions on connected displays, including displays left of or above the primary display.
If the saved display is disconnected, center Peek on the main window's current display and reduce its size if necessary.
Held Peek appears without activating Keyclash or taking keyboard focus.
Closing Peek preserves the app that had focus when it opened, independently of which app's shortcuts it shows.
A quick double tap still gives Peek keyboard focus. Running a shortcut explicitly activates its target app.

### Chips and segmented controls

Chips expose checkbox names and states. Segmented choices expose radio-button names and states.
Custom controls receive the shared focus outline.

### Keyboard map

Key shapes preserve physical positions while character labels follow the active keyboard layout.
Each interactive key exposes the full combination, status, app count, and press count.
Modifier keys expose their names and selected states. The keyboard stays in the left pane while matching shortcuts scroll in the right pane.
The shortcut heading and count stay visible. Modifier-held wheel input scrolls the list without zooming the interface.
Pressing a full shortcut or clicking a key while holding modifiers selects the complete combination. The selection stays visible after key release. A new modifier press or modifier click clears the key filter and shows all matching shortcuts.
Every key outline keeps 3:1 against the card and the key's fill. In the dark theme, shared keys use a lighter outline and fill to distinguish them from free keys.
Unused keys use `key_line`. The small key caps in shortcut rows keep the quieter `keycap_line`.
Heat-map outlines change from orange to red as the press count rises. They keep 3:1 contrast against the card and the fill. Labels keep 4.5:1 contrast at every press count.
Hover thickens the status outline. Selection adds an outer ring in the text color, 1 point away from the outline.
Keyboard focus adds an inset ring in the text color on every map key, including modifiers. A gap separates it from the status outline.
The small count on a key uses the label color.
Tests in `keyboard.rs` and `use_tab.rs` check these ratios in both themes.

### Cards and shortcut rows

Keep the action, app, scope, source, and uncertainty readable together.
Show active conflicts first. Inactive conflicts remain available through the explicit filter.
Blocked-shortcut cards name the blocking app beside the keys. Their explanation states when its global shortcut takes priority and how to use the affected action.
Use "May be blocked by" when the blocker is estimated or not running. Wrap this explanation so it remains visible.
Key combinations expose one complete spoken label rather than isolated painted symbols.

## Do's and Don'ts

- Preserve the compact interface and established icon.
- Use the active theme and shared controls.
- Check keyboard focus and macOS accessibility output after custom painting changes.
- Keep uncertain usage matches separate from uniquely matched shortcuts.
- Do not treat the sidecar's browser previews as native UI validation.
- Do not introduce web or iOS navigation patterns into this macOS app.
