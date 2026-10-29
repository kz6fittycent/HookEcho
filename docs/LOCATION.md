# "Use my location": proposal (2026-10-28)

**Decided 2026-10-29: option A, built** (`hookecho-app::location`, Menu → "Use my location"). Not run once at first start; the owner can still ask for that.

Owner fix 2 left one piece open: setting home from the computer's own location instead of clicking the map. This is the research and the proposal. **Nothing is built until the owner picks an option.**

## What's there today

- Menu → Set home (click the map) and Menu → Nearest radar (PR #18).
- The snap already declares the `location-observe` plug (`snap/snapcraft.yaml`), but no code uses it.

## Options

| | A. Location portal (recommended) | B. GeoClue directly | C. Keep it manual |
|---|---|---|---|
| How | `org.freedesktop.portal.Location` through xdg-desktop-portal, which asks GeoClue on the host | Talk to GeoClue2 over the system bus | Click the map (today) |
| Rust | `ashpd` 0.13 (MIT), `location` feature. It is built on `zbus` 5, which is already in our tree through accesskit, so the cost is ashpd itself | `zbus` proxies written by hand | none |
| Snap | Works through the `desktop` plug we already have; the desktop shows its own permission prompt | Needs `location-observe`, which is **not** auto-connected: users would have to run `snap connect hookecho:location-observe`, or we'd have to ask the store for auto-connection | none |
| Privacy | The user grants it in a system dialog; GNOME Settings → Privacy → Location can revoke it | Bypasses the desktop's prompt | Nothing leaves the app |
| Accuracy | Street-level is plenty for "nearest radar" | Same source | Exact |
| Where it fails | Location services off, or no GeoClue (some minimal desktops): say so and fall back to clicking the map | Same, plus the manual connect | n/a |

GeoClue gets its fix from Wi-Fi networks and, on some distributions, from an online lookup service the distribution configures. Hook Echo itself would add no endpoint either way.

## Proposal

Option A:

1. Add `ashpd` with only the `location` feature (and its tokio integration), on the app side, behind one function: `locate() -> Result<(lat, lon), Why>`.
2. Add Menu → "Use my location": one request at street accuracy, sets home, then goes to the nearest radar. No background tracking, and nothing is asked for at startup.
3. When it fails, a notice says why ("Location is turned off in Settings", "No location service on this computer") and offers Set home.
4. Drop the unused `location-observe` plug from the snap, so the store listing doesn't claim location access we don't use.
5. PROVENANCE: the XDG Location portal spec. THIRD_PARTY_LICENSES: ashpd.

The owner's call: A, B, or C, and whether "Use my location" should also run once on the very first start (as a prompt, not automatically).
