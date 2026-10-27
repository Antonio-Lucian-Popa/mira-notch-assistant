# Mira — landing page

A single-page, dependency-free marketing site for Mira. Pure static files, ready
to deploy to Netlify (or any static host).

```
website/
├─ index.html      # all the sections
├─ styles.css      # theme + layout (tokens mirror design/tokens.json)
├─ script.js       # cursor-following eyes, confetti, accessible tabs, reminder / routine previews
├─ netlify.toml    # static config (no build step)
└─ assets/
   ├─ favicon.svg
   └─ og.svg       # social preview card
```

## Preview locally

No build needed — just serve the folder:

```sh
cd website
python3 -m http.server 8080
# open http://localhost:8080
```

(or just open `index.html` directly in a browser.)

## Deploy to Netlify

**Option A — drag & drop (fastest):**
Open <https://app.netlify.com/drop> and drop the `website/` folder. Done.

**Option B — connect the Git repo (auto-deploys on push):**
1. Netlify → *Add new site* → *Import an existing project* → pick this repo.
2. Set **Base directory** to `website`.
3. Leave **Build command** empty and **Publish directory** as `.`.
4. Deploy.

## Customise

- Colors and spacing live as CSS variables at the top of `styles.css` and track
  `design/tokens.json`.
- Update the GitHub URL in `index.html` if the repo moves.
- Replace `assets/og.svg` with a 1200×630 PNG if you want richer link previews
  (some platforms don't render SVG `og:image`).

## Current presentation

The site now covers seven GNOME pages, the standalone countdown reminder demo,
manual lyric lookup for browser/private-session metadata, and interactive lunch /
end-of-work greeting examples. It distinguishes the Windows preview feature set
from GNOME and links to actual GitHub Releases rather than promising parity.
Demo times and tracks are examples; the site does not create real reminders,
inspect playback, or send lyric queries. Runtime/private-browser limitations are
explained in the FAQ. Verify the tabs, urgency buttons, routine buttons, keyboard
navigation, mobile wrapping and reduced-motion mode when changing the demos.
