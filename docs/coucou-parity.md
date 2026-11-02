# Coucou → Mira: analiză și plan de paritate

Analiză a [Coucou](https://github.com/louis-cfm/coucou) (v0.5.8) și cum aducem
aceleași capabilități în **Mira**, cu grafica și ochii noștri originali.

> **Licență / proprietate intelectuală.** Codul Coucou e **MIT** (îl putem citi
> și chiar reutiliza cu păstrarea notei de copyright). DAR numele „Coucou",
> personajul **Mochi**, iconița, **sunetele** și media sunt **© Louis Raillé,
> all rights reserved**. README-ul lor spune explicit: *"Shipping your own fork?
> Give it your own name and character."* → Mira folosește **mascota, ochii,
> numele și sunetele proprii**. Replicăm *mecanica/funcțiile*, nu grafica lor.

---

## 1. Ce este Coucou

Un „companion de notch / dynamic island" care stă în notch-ul Mac-ului (sau la
marginea de sus pe Windows/Linux) și, în esența lui, **supraveghează sesiunile
agenților AI de cod** (Claude Code, Codex, Cursor, Gemini CLI, Copilot CLI etc.):
vezi live ce citește/editează/rulează agentul, **aprobi permisiunile** și
**răspunzi la întrebări** direct din notch, fără să pleci din ce faci. Peste asta
a crescut o grămadă de funcții: chat AI, WhatsApp, dictare, notetaker, mail,
integrări de servicii, portofoliu crypto, companion pe desktop, plus aplicații
de **iPhone** (Live Activity / Dynamic Island) și **Android**.

### Tech stack
- **macOS**: Swift 6 / SwiftUI / AppKit, **zero dependențe terțe**. Insula =
  `NSPanel` fără bordură; personajul desenat în SwiftUI `Canvas` + `TimelineView`
  la 60fps (corp squircle, ochi proiectați pe o sferă, animații spring — fără
  Rive/Lottie/imagini).
- **Windows + Linux**: aceeași aplicație **Tauri 2** (Rust + TypeScript); insula =
  fereastră transparentă always-on-top; pe Wayland e un overlay gtk-layer-shell
  ancorat sus. Mascota desenată în Canvas 2D cu aceleași forme/timings.
- **Android**: Kotlin + Jetpack Compose.
- **Hooks agenți**: un script mic (`coucou-hook`) primește evenimentele de hook
  și le trimite aplicației prin Unix socket (Mac/Linux) / named pipe (Windows).
  Dacă aplicația nu rulează, hook-ul iese imediat → **agentul nu e blocat**.
- **Legătura cu telefonul**: server opt-in pe rețeaua locală, TLS cu certificat
  pinned la pairing + cod QR (exact modelul pe care Mira îl are deja în `bridge`).

---

## 2. Inventar complet de funcții (Coucou)

**Nucleu — monitorizare agenți AI**
- Sesiuni live în notch pentru Claude Code, Cursor, Codex, Gemini CLI,
  Antigravity, Copilot CLI, Muse, OpenCode, Amp, Hermes, Claude Desktop (+ orice
  agent prin `--agent <name>`). Vezi pas cu pas ce citește/editează/rulează.
- Diff live: fiecare modificare de fișier arată numele + `+N −M`, tap → diff complet.
- **Aprobare din notch**: Allow / Deny / Always la permisiunile Claude Code;
  `AskUserQuestion` cu variante (1–4 întrebări, single/multi-select). Codex: Allow/Deny.
- „Jump to terminal", plan usage Claude (5h/săptămânal) și Codex.

**Pills / tile-uri**
- Catalog de „pills" colorate (câte una per unealtă/agent/serviciu), declarate
  într-un singur loc (`PillCatalog.swift`). La hover ies pe un sfert de cerc.
- „Active pills" în setări: alegi unealta principală + până la 4 extra.

**Chat AI**
- Chat cu Claude, Gemini, OpenAI, DeepSeek, Qwen, Moonshot sau model local
  (Ollama / LM Studio). Click pe numele modelului → schimbi providerul.
- Cheia ta API SAU — nou — **pe abonamentul tău** prin uneltele CLI oficiale pe
  care deja le plătești (Claude Code, Gemini CLI, Codex/ChatGPT, Cursor, Copilot).

**Comunicare**
- **WhatsApp** (experimental) prin gateway local **GOWA** (sau OpenWA): „Install
  WhatsApp" descarcă binarul, verifică SHA-256, pornește, scanezi QR; one-line-per-chat,
  reply din notch, grupuri tăcute dacă nu ești menționat. Server local pe
  `127.0.0.1` cu secret HMAC, fallback poll la 20s.
- **Mail în notch** (Apple Mail / Outlook): căutare, filtre, citire, reply, sortare
  automată cu reguli + Undo; „mail plans" din chat.

**Voce**
- **Flow dictation**: ții o tastă oriunde, vorbești, textul se lipește în orice
  app (ca Wispr Flow). Model offline **Parakeet** pe Neural Engine (25 limbi).
- **Notetaker**: înregistrează apelul (mic + ce redă Mac-ul, fără bot), transcrie
  pe persoane, sumar, puncte-cheie, to-dos; audio-ul se șterge după.
- **„OK Coucou"**: wake-word, vorbești cu mascota ca cu Jarvis.
- Dictare în chat.

**Caracter & desktop**
- Mascotă cu respirație idle, clipit, ochi pe sferă care urmăresc cursorul,
  emotes, 28 sunete, salut la pornire, „se enervează" la click, amețeală la 3 click-uri.
- **Pe desktop / în colț**: tragi mascota afară din insulă → plutește 120pt,
  urmărește cursorul; sau o pui în orice colț (tile mic, pills pe sfert de cerc).
- Drop fișier pe insulă → „înghite" → întrebi AI-ul despre el / trimiți pe mail.
- Garderobă/ținute, sezoane.

**Integrări de servicii (fiecare = un pill)**
- GitHub (PR-uri, review-uri cerute, CI), Stripe, Vercel, Resend, Notion,
  Cal.com, n8n. Music: Spotify + Apple Music (cover, seek, play/pause).
- **Portfolio crypto**: 16 rețele + 5 exchange-uri, solduri live + sparklines.

**Mobil**
- iPhone: Live Activity + Dynamic Island, widgets, Siri/Shortcuts, Face ID la
  fiecare Allow, sync prin CloudKit privat.
- Android: aprobare cu amprentă, island peste alte app-uri, 9 widgets, legătură
  LAN cu QR + certificat pinned.

**Platformă & confort**
- Invizibil când nu rulează nimic, iese la hover. Orice Mac (cu/fără notch).
- Update în notch (un click). 16 limbi. Căutare în setări. Shortcut-uri custom.
- **Private by design**: fără telemetrie, fără cont; cheile în Keychain /
  Credential Manager / Secret Service.

---

## 3. Ce are deja Mira (bază solidă de pornire)

| Capabilitate | Stare în Mira |
|---|---|
| Notch Windows (GDI nativ) + Linux/GNOME | ✅ |
| Muzică + versuri (MPRIS/LRCLIB), Pomodoro, apă, remindere, todo, shelf, clipboard, căutare, notițe, rezumat zilnic | ✅ |
| Mascotă cu ochi (urmăresc cursorul, clipit, teme classic/robot/cat/pixel, mood contextual) | ✅ |
| WhatsApp prin GOWA (install + QR real + chat) — GNOME complet, Windows worker | ✅ (din munca recentă) |
| Chat AI pluggable (Ollama / OpenAI / Anthropic), cheia ta sau local | ✅ |
| Bulină în colțul desktopului (GNOME) | ✅ |
| Bridge LAN cu pairing QR + certificat pinned (pentru acțiuni de pe telefon) | ✅ |
| Local-first, fără telemetrie, fără cont | ✅ |
| Bilingv RO/EN; preview Android cu voce | ✅ |

Arhitectura Mira (daemon + event bus + D-Bus pe Linux / host in-process pe
Windows + bridge LAN) e **exact** forma potrivită pentru ce face Coucou.

---

## 4. Gap analysis — ce lipsește și cum îl facem în Mira

Ordonat după valoare/efort. „Efort" e orientativ.

### A. NUCLEUL: monitorizare agenți AI + aprobare din notch  ⭐ (efort: mare, valoare: maximă)
Asta e inima lui Coucou și lipsește complet din Mira.
- **Hook relay**: un binar mic `mira-hook` (sau script) pe care îl instalăm în
  `~/.claude/settings.json` (cu backup + diff înainte de scriere, ca ei). Primește
  evenimentele de hook Claude Code (`PreToolUse`, `PostToolUse`, `Notification`,
  `Stop`, `SessionStart`…) și le trimite daemonului:
  - Linux: prin D-Bus sau Unix socket în `$XDG_RUNTIME_DIR` (avem deja D-Bus).
  - Windows: named pipe către host (avem deja worker-ul de pattern).
- **Model nou în `mira-core`**: `AgentSession { id, agent, cwd, steps: Vec<Step>,
  state (running/waiting/done), pending: Option<Approval|Question> }`. Reutilizăm
  event bus-ul (`AppEvent::Agent(...)`).
- **Pagină nouă „Agents"** (GNOME + Windows) + un pill per sesiune: arată ce
  face agentul, diff-uri (`+N −M`), și — crucial — **Allow / Deny / Always** și
  `AskUserQuestion` cu variante. Hook-ul așteaptă click-ul, apoi răspunde.
  Dacă Mira nu rulează → hook-ul iese imediat (agentul nu e blocat).
- **Extensibil**: `--agent <name>` pentru Codex/Gemini/Cursor etc. (adaptoare).
- Se leagă frumos de **bridge-ul LAN** → aprobare de pe telefon (Mira are deja
  infra de pairing).

### B. Sistem de „pills" (tile-uri) + hover pe sfert de cerc  (efort: mediu)
- Un `PillCatalog` în `mira-core`/tokens: fiecare pill cu id, culoare, categorie
  (agenți, AI, servicii, media). „Active pills" în setări.
- Animația „hover → celelalte pills ies unul câte unul" (ca în pozele trimise).
  Pe GNOME (Clutter) și Windows (GDI) o putem face cu grafica nouă.

### C. Integrări de servicii (pills)  (efort: mediu-mare, modular)
- **GitHub** (PR-uri deschise, review-uri cerute, status CI) — primul, cel mai util.
- Apoi, opțional și modular: Vercel, Stripe, Notion, Cal.com, n8n, Resend.
- Fiecare = un poller ușor, pus pe pauză când nu se uită nimeni (ca ei).

### D. Chat AI „pe abonament" prin CLI-urile oficiale  (efort: mic-mediu)
- Pe lângă cheia API / Ollama (pe care le avem), adăugăm opțiunea de a trimite
  chat-ul prin **Claude Code CLI / Codex / Gemini CLI** instalate local (procesul
  oficial pe care userul deja îl plătește). Asta rezolvă exact întrebarea ta de
  la început („pot folosi abonamentul?") — legitim, prin unealta oficială, nu
  prin reverse-engineering de browser.

### E. Voce: dictare „Flow" + „OK Mira"  (efort: mare)
- Dictare globală (ții o tastă, vorbești, se lipește). Pe Linux: `ydotool`/uinput
  pentru paste; recunoaștere prin Vosk/whisper.cpp offline. Avem deja voce pe
  Android preview — refolosim conceptul.
- Wake-word „OK Mira" → folosește chat-ul AI existent ca creier.

### F. Mail în notch  (efort: mare) — probabil mai târziu
- Linux: IMAP direct; Windows: MAPI/Graph. Căutare, filtre, reply, sortare cu reguli.

### G. Notetaker (înregistrare apel + transcript + sumar)  (efort: mare) — mai târziu

### H. Confort / platformă  (efort: mic, valoare bună)
- Update în notch (un click) — avem deja infra de release.
- Căutare în setări. Mai multe limbi. Shortcut-uri custom. Sunete (originale!).
- Portfolio crypto, drop-file→AI — nice-to-have.

### I. Mobil  (efort: mare) — avem deja bridge + preview Android
- Extindem preview-ul Android: aprobări cu amprentă, island peste alte app-uri,
  widgets. iPhone (Live Activity) ar fi un proiect separat mare.

---

## 5. Grafica nouă + ochii îmbunătățiți (originali, nu Mochi)

Coucou desenează ochii „proiectați pe o sferă" cu animații spring, în Canvas, la
60fps, fără imagini. Mira deja desenează ochii în cod (Clutter/GDI). Propuneri de
**îmbunătățire a ochilor Mira**, păstrând identitatea proprie:
- **Ochi pe sferă**: proiecție pseudo-3D (parallax ușor) ca privirea să pară
  volumetrică când urmărește cursorul — dar cu stilul Mira, nu al lui Mochi.
- **Fizică spring** pe privire, clipit și „squish" la click; micro-saccade când e idle.
- **Emotes legate de evenimente**: salt de bucurie când un agent termină,
  privire „alertă"/sprânceană când o permisiune așteaptă, amețeală la spam de click.
- **Corp squircle** reîmprospătat în tokens (`design/tokens.json`) + o paletă nouă,
  consistentă pe GNOME și Windows (tokens sunt deja sursa unică de adevăr).
- Toate acestea stau în `mascot` (tokens) + randarea din `notch.js` / `host.rs`,
  fără să stricăm notch-ul docked.

---

## 6. Roadmap propus (faze)

1. **Faza 1 — Nucleul agenți** (A): hook relay + model + pagină „Agents" +
   Allow/Deny/Always + AskUserQuestion. Întâi Claude Code, întâi GNOME, apoi Windows.
   → Asta transformă Mira într-un „Coucou open" real.
2. **Faza 2 — Pills + GitHub** (B + C-GitHub) + **chat pe CLI oficial** (D).
3. **Faza 3 — Ochi/grafică nouă** (secțiunea 5) + confort (H: update-in-notch,
   căutare setări, shortcut-uri, sunete originale).
4. **Faza 4 — Voce** (E: dictare + „OK Mira").
5. **Faza 5 — Integrări extra** (C restul), mail (F), notetaker (G), mobil (I).

**Onest despre amploare:** Coucou e un produs matur, multi-platformă, cu zeci de
integrări și 3 aplicații (Mac/iPhone/Android). „Tot ce are el" = multe luni de
lucru. Dar Mira pleacă de la o bază bună și **nucleul** (monitorizarea agenților +
aprobări) e foarte fezabil pe arhitectura actuală — ăla aduce 80% din identitate.

---

*Sursă: [github.com/louis-cfm/coucou](https://github.com/louis-cfm/coucou) —
README v0.5.8. Rezumat pentru planificare; nu copiem grafica/personajul/sunetele lor.*
