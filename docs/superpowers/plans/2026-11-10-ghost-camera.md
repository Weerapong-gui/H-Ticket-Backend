# Ghost Camera Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the home-page mock camera with a real in-browser camera that composites a faint "spirit photo" ghost behind the person, offers a download, and counts shots on the backend.

**Architecture:** Everything image-related runs in the visitor's browser: `getUserMedia` → one still → MediaPipe selfie segmentation (self-hosted wasm + model) → canvas compositing (photo → ghost via screen blend → person cut-out on top → film grade). Pure placement/mask math lives in `lib/ghost-cam/*` and is tested with the existing `scripts/verify-*.cjs` pattern. The backend only gains a public `+1` counter in `settings` and exposes it in admin stats. The old `FlashCamera` stays as the fallback.

**Tech Stack:** Next.js 16.4.5 / React 19 / TypeScript (frontend, read `node_modules/next/dist/docs/` before touching Next APIs), `@mediapipe/tasks-vision@1.1.1` (pinned; ≥2 weeks old), canvas 2D; Rust/Axum/sqlx/Postgres (backend).

**Spec:** `docs/superpowers/specs/2026-11-10-ghost-camera-design.md` (this repo)

**Repos:** backend `/Users/park/Projects/haunted_house/H-Ticket-Backend`, frontend `/Users/park/Projects/haunted_house/h-ticket-frontend`. Both on branch `feat/no-ticket-numbers`. Commit with `git -c user.name=PARK -c user.email=park@PARKs-MacBook-Air.local commit …` and end messages with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Backend tests need `DATABASE_URL=postgres://hticket:$POSTGRES_PASSWORD@localhost:5432/hticket` (password from `.env`).

## Global Constraints

- Photos never leave the device; no request carries image data. The only network call is `POST /api/ghost-shots` with an empty body.
- MediaPipe wasm + `selfie_segmenter.tflite` are served from our own `public/`, never fetched from Google at runtime; loaded lazily when the user taps "เปิดกล้อง".
- Ghost sprites are original AI-generated images on pure black, stored in `public/ghosts/`; never the files in `~/Downloads/Haunted pic` (film stills — reference only).
- Ghost is faint and light ("spirit photo"), blended with `screen`; it must sit behind the person (person layer drawn over it) and must never cover the face.
- Live-camera shots always contain a ghost; the fallback mock camera keeps its 50 % rule.
- `ghost_shots` counts every shutter press in both modes; counter failures never block or alter the photo flow.
- Flash rule stays: no faster than one shot per 1.5 s (`canShoot`), weaker flash under reduced motion.
- When `figure.mirrorArmed` the next live shot uses the black figure right behind the person, then `consumeMirror()`.
- Copy follows the recent de-template rules: no dot markers, no eyebrow labels, no half-italic headings, no ✦.
- Must ship before Sat 14 Nov 2569. Cut order if late: hand-on-shoulder first, then far-in-scene. Must ship: behind-shoulder, screen-edge, black-figure ending.

## Review Focus

1. Camera unavailable (permission denied, no device, insecure http, old browser) → falls back to the mock `FlashCamera` with a one-line notice, never a blank frame. *(Task 5 test)*
2. Segmenter fails to load or throws on a device → photo still produced with the screen-edge variant. *(Task 3 + Task 5 tests)*
3. Person fills the frame or stands at an edge → ghost goes to the side with room, otherwise screen-edge; ghost box never overlaps the head box. *(Task 2 tests)*
4. Leaving the section / unmounting / switching cameras → all `MediaStreamTrack`s stopped (camera light off). *(Task 5 test)*
5. Counter endpoint down or rate-limited (429) → shot flow identical, no error shown. *(Task 1 + Task 5 tests)*

---

### Task 1: Backend ghost-shot counter

**Files:**
- Modify: `src/db/settings.rs` (add counter fns)
- Modify: `src/routes/public.rs` (add handler), `src/routes/admin.rs` (`stats` adds field), `src/lib.rs` (route + limiter in `AppState`), `src/auth.rs` or new `src/limit.rs` (limiter)
- Test: `tests/admin_api.rs`, `tests/issue.rs`
- Modify: `README.md` (API table)

**Interfaces:**
- Produces: `POST /api/ghost-shots` → `204 No Content` (no auth, empty body); `429 too_many_attempts` when over the global rate. `GET /api/admin/stats` gains `"ghost_shots": <i64>`.
- `settings::add_ghost_shot(pool: &PgPool) -> Result<(), AppError>`; `settings::ghost_shots(pool: &PgPool) -> Result<i64, AppError>` (0 when unset). Stored as settings key `ghost_shots` (text integer), incremented with a single `INSERT … ON CONFLICT DO UPDATE SET value = (settings.value::bigint + 1)::text`. Does **not** take `ISSUE_LOCK`.
- `ShotLimiter` in `AppState` (`Arc`, `Default`): global fixed window, at most 30 hits per rolling second; `check(&self) -> Result<(), AppError>` returns `AppError::TooManyAttempts { retry_after_secs: 1 }`.

- [ ] **Step 1: Write failing tests**
  - `tests/issue.rs::ghost_shots_count_up`: fresh DB → `ghost_shots == 0`; `add_ghost_shot` ×3 → `3`.
  - `tests/admin_api.rs::ghost_shot_counter_public_and_in_stats`: two `POST /api/ghost-shots` without token → each `204`; admin stats `ghost_shots == 2`.
  - `tests/admin_api.rs::ghost_shot_counter_rate_limited`: 40 rapid posts → at least one `429` with `code == "too_many_attempts"`; stats `ghost_shots <= 30`.
  - Add `("POST", "/api/ghost-shots")` is **not** in `admin_routes_require_token` (public).
- [ ] **Step 2: Run** `cargo test ghost_shot` → FAIL (missing fns/route).
- [ ] **Step 3: Implement** the interfaces above; register `.route("/api/ghost-shots", post(routes::public::ghost_shot))`.
- [ ] **Step 4: Run** full `cargo test` → all pass (was 68 + 3 new); `cargo clippy --all-targets` clean; `cargo fmt`.
- [ ] **Step 5: README** — add the endpoint row and `ghost_shots` to the stats row.
- [ ] **Step 6: Commit** `feat: public ghost-shot counter in admin stats`.

### Task 2: Mask stats + ghost placement (pure, frontend)

**Files:**
- Create: `lib/ghost-cam/mask.ts`, `lib/ghost-cam/placement.ts`, `lib/ghost-cam/ghost-config.ts`
- Create: `scripts/verify-ghost.cjs`; Modify: `package.json` (`"test:ghost": "node scripts/verify-ghost.cjs"`)

**Interfaces:**
- Produces (`mask.ts`):
  ```ts
  export type Box = { x: number; y: number; w: number; h: number }; // pixels
  export type MaskStats = {
    person: Box;          // bbox of pixels with confidence ≥ threshold
    head: Box;            // top 22 % of person.h, centred on the topmost row's span
    coverage: number;     // person pixels / all pixels, 0–1
    freeLeft: number;     // background width left of person, px
    freeRight: number;    // background width right of person, px
  };
  export function maskStats(mask: Float32Array, w: number, h: number, threshold = 0.5): MaskStats | null; // null if coverage < 0.02
  ```
- Produces (`ghost-config.ts`):
  ```ts
  export type GhostKind = "behind" | "far" | "edge" | "hand";
  export type GhostSprite = { id: string; src: string; kind: GhostKind; flippable: boolean };
  export const ghostSprites: GhostSprite[]; // filled in Task 4; Task 2 ships one placeholder per kind using src "procedural:<kind>"
  ```
- Produces (`placement.ts`):
  ```ts
  export type Variant = "behind-left" | "behind-right" | "far" | "edge" | "hand" | "figure";
  export type Placement = { variant: Variant; sprite: GhostSprite | null; box: Box; flipX: boolean; opacity: number; blurPx: number };
  export function planGhost(stats: MaskStats | null, w: number, h: number, sprites: GhostSprite[], opts: { figure: boolean; rand: () => number }): Placement;
  ```
  Rules: `figure` → variant `"figure"`, sprite `null`, box behind the person's larger free side at 1.05× person height, bottom aligned to person bottom (if `stats` null → centred, 0.8 h). Else if `stats` null or both free sides < 0.18 w → `"edge"`. Else random among `behind-left/right` (only sides with free ≥ 0.18 w), `far`, `hand` (weights 5/5/2/1, `hand` only if a `hand` sprite exists). Behind box height 0.9–1.1× person.h, overlapping the person bbox by 15–30 % of box width on that side. `far` box height 0.25–0.4× person.h, top within the upper third, inside the larger free side. `edge` box height 0.45–0.6 h, hugging the left or right image edge with 40–60 % of its width outside the frame. Opacity 0.28–0.45, `blurPx` 1.5–4 (figure: opacity 0.85, blur 2). **No box except `hand` may intersect `stats.head`** — shrink/shift away from the head; if impossible fall back to `"edge"`.

- [ ] **Step 1: Write `scripts/verify-ghost.cjs`** (same `load()` transpile helper as `verify-scare.cjs`) with assertions:
  - `maskStats` on a 100×100 mask with a filled rect x 40–59, y 30–99 → `person` `{x:40,y:30,w:20,h:70}`, `freeLeft 40`, `freeRight 40`, `coverage ≈ 0.14`; empty mask → `null`.
  - `planGhost(null, 600, 800, sprites, {figure:false, rand})` → `variant "edge"`.
  - person filling 90 % width → `"edge"` (Review Focus 3).
  - person at the left edge (free right only) over 200 seeded rands → never `"behind-left"`.
  - over 500 seeded rands with a centred person: no non-`hand` placement intersects `head`; every opacity in [0.28, 0.45].
  - `figure:true` → `variant "figure"`, `sprite null`, box bottom = person bottom.
  - `hand` never chosen when no `hand` sprite in the list.
- [ ] **Step 2: Run** `npm run test:ghost` → FAIL (modules missing).
- [ ] **Step 3: Implement** the three modules (seeded `rand` injected; no `Math.random` inside).
- [ ] **Step 4: Run** `npm run test:ghost` → pass; `npx tsc --noEmit`, `npx eslint lib scripts` clean.
- [ ] **Step 5: Commit** `feat: ghost placement and mask stats`.

### Task 3: Segmenter + compositor (browser)

**Files:**
- Modify: `package.json` (dependency `"@mediapipe/tasks-vision": "1.1.1"`; scripts `"sync:mediapipe": "node scripts/sync-mediapipe.cjs"`, `"predev"` and `"prebuild"` run it)
- Create: `scripts/sync-mediapipe.cjs` — copies `node_modules/@mediapipe/tasks-vision/wasm/*` → `public/mediapipe/wasm/` (idempotent)
- Create: `public/mediapipe/selfie_segmenter.tflite` (download once from `https://storage.googleapis.com/mediapipe-models/image_segmenter/selfie_segmenter/float16/latest/selfie_segmenter.tflite` and commit; ~250 KB)
- Modify: `.gitignore` (`/public/mediapipe/wasm/`)
- Create: `lib/ghost-cam/segmenter.ts`, `lib/ghost-cam/compose.ts`

**Interfaces:**
- `segmenter.ts`: `export async function segmentPerson(src: HTMLCanvasElement): Promise<{ mask: Float32Array; w: number; h: number } | null>` — lazily creates one `ImageSegmenter` (`FilesetResolver.forVisionTasks("/mediapipe/wasm")`, model `/mediapipe/selfie_segmenter.tflite`, `runningMode: "IMAGE"`, `outputConfidenceMasks: true`), returns the person confidence mask; **never throws** (any failure → `null`, logs once). `export function preloadSegmenter(): void` (fire-and-forget).
- `compose.ts`: `export async function composeGhostPhoto(photo: HTMLCanvasElement, seg: {mask,w,h} | null, placement: Placement, sprite: CanvasImageSource | null): Promise<HTMLCanvasElement>` — output 1080×1440 (3:4). Order: (1) photo; (2) ghost: sprite (or `drawGhost` from `lib/scare/ghost-shape.ts` for `figure`/`procedural:*`) drawn into `placement.box`, flipped if `flipX`, colour-matched to mean RGB/luma of the photo region under the box (scale ghost brightness so its peak ≈ background mean + 35 %), blurred via downscale→upscale by `blurPx`, composited with `globalCompositeOperation = "screen"` at `placement.opacity` (`figure` uses `"source-over"` dark fill); (3) person layer: photo masked by the upscaled, slightly feathered confidence mask drawn over everything **except** when `variant === "hand"`; (4) grade the whole image: desaturate ~25 %, cool tint, film grain (reuse the `grain` approach in `flash-camera.tsx`), radial vignette. `export function canvasToJpegBlob(c: HTMLCanvasElement, q = 0.9): Promise<Blob>`.

- [ ] **Step 1:** `npm install @mediapipe/tasks-vision@1.1.1 --save-exact`; add sync script + pre-hooks; download the model; `npm run sync:mediapipe` → `public/mediapipe/wasm/vision_wasm_internal.wasm` exists.
- [ ] **Step 2:** Implement `segmenter.ts` and `compose.ts` to the signatures above (no unit tests — canvas/wasm; verified in Task 5's browser check). Keep pure helpers (colour-match maths, box flip) exported and add assertions for them to `scripts/verify-ghost.cjs`: `matchBrightness(ghostPeak 255, bgMean 40)` → scale so result peak ≈ 54 (40 × 1.35); flipping a box mirrors `x` around image width.
- [ ] **Step 3: Run** `npm run test:ghost`, `npx tsc --noEmit`, `npx eslint lib scripts`, `npm run build` → all pass, build output contains `public/mediapipe/wasm`.
- [ ] **Step 4: Commit** `feat: in-browser person segmentation and ghost compositor`.

### Task 4: Ghost sprites (AI, one-off) + owner review checkpoint

**Files:**
- Create: `public/ghosts/*.webp` (or `.jpg`), Modify: `lib/ghost-cam/ghost-config.ts` (replace placeholders)

- [ ] **Step 1:** Generate with an available image tool (Canva `generate-image` or Figma `generate_image`; load via ToolSearch). Every prompt ends with: *"isolated on a pure black background, faint pale spirit-photograph look, no text, no frame, original character"*. Sprites (kind):
  1. long black hair, decayed pale face, white eyes, chest-up, facing camera (`behind`)
  2. long hair fully covering the face, standing still, full body, white robe (`far`)
  3. pale face in Thai classical dancer white make-up wearing a tall golden chada crown, half face peeking in from the left (`edge`)
  4. rotten-faced spirit in a tarnished chada crown and old Thai costume, chest-up (`behind`)
  5. crouching long-haired woman spirit, full body (`far`)
  6. pale thin hand reaching down, wrist up (`hand`)
- [ ] **Step 2:** Post-process with Pillow (`python3 -I`): crop to content, crush near-black to pure black, resize ≤ 900 px, save `.webp`; set `kind`, `flippable`.
- [ ] **Step 3:** Run `npm run test:ghost` (config shape assertions) → pass.
- [ ] **Step 4: Owner checkpoint** — show the sprites (and, after Task 5, live composites) to the user; replace any they reject. Do not ship sprites they have not seen.
- [ ] **Step 5: Commit** `feat: original ghost sprites for the spirit camera`.

### Task 5: GhostCamera UI, counter call, fallback, admin

**Files:**
- Create: `components/scare/ghost-camera.tsx`; Modify: `app/page.tsx` (render `GhostCamera` in `#camera` instead of `FlashCamera`), `components/scare/flash-camera.tsx` (call counter on shot; drop its `<h2>` when rendered inside GhostCamera via a `showTitle?: boolean` prop, default `true`), `app/globals.css`
- Modify: `lib/api.ts` (`countGhostShot`, `AdminStats.ghost_shots`), `lib/api-contract.ts` (`ghost_shots` optional count), `app/admin/page.tsx` (stat tile "ถ่ายรูปผี")
- Create: `lib/ghost-cam/camera-flow.ts` (pure state machine); Test: `scripts/verify-ghost.cjs`, `scripts/verify-api.cjs`

**Interfaces:**
- `api.countGhostShot(): void` — `fetch POST ${API_URL}/api/ghost-shots`, `keepalive: true`, swallows every error, never awaited by callers.
- `camera-flow.ts`:
  ```ts
  export type CamState = "idle" | "starting" | "live" | "developing" | "result" | "unavailable";
  export type CamEvent = "open" | "granted" | "denied" | "shoot" | "developed" | "retake" | "close";
  export function nextCam(s: CamState, e: CamEvent): CamState;
  ```
  `idle-open→starting`, `starting-granted→live`, `starting-denied→unavailable`, `live-shoot→developing`, `developing-developed→result`, `result-retake→starting`, any `close→idle`; anything else returns `s` unchanged.
- `GhostCamera`: heading "ถ่ายรูปกับผีในบ้านนี้"; button "เปิดกล้อง"; live 3:4 `<video playsInline muted>` (front camera mirrored with CSS); buttons "ชัตเตอร์" and "สลับกล้อง"; developing overlay "กำลังล้างฟิล์ม…" shown ≥ 2000 ms (or until compose finishes); result `<img>` + "ดาวน์โหลดรูป" (`<a download="haunted-house-ghost.jpg">` with an object URL, revoked on retake/unmount) + "ถ่ายใหม่". On `unavailable` show "เปิดกล้องไม่ได้ ลองถ่ายบ้านแทน" and render `<FlashCamera showTitle={false} />`. Calls `preloadSegmenter()` on "เปิดกล้อง". On shoot: `canShoot` gate, flash overlay, draw current video frame (mirrored for front cam) to a 1080×1440 centre-cropped canvas, stop stream tracks, `api.countGhostShot()`, `segmentPerson` → `maskStats` → `planGhost({figure: figure.mirrorArmed, rand: Math.random})` → load sprite → `composeGhostPhoto`; if `figure` then `consumeMirror()`. Stops all tracks on unmount, on retake (before restarting) and on camera switch.

- [ ] **Step 1: Failing tests** — in `verify-ghost.cjs`: full transition table of `nextCam` (every listed edge plus `idle-shoot → idle`, `developing-shoot → developing`). In `verify-api.cjs`: `countGhostShot()` posts to `/api/ghost-shots` with `method "POST"` and no body; with `fetch` rejecting it does not throw (Review Focus 5); `isAdminStats` accepts payload with and without `ghost_shots` and rejects `ghost_shots: -1`.
- [ ] **Step 2: Run** `npm run test:ghost && npm run test:api` → FAIL.
- [ ] **Step 3: Implement** `camera-flow.ts`, api additions, `GhostCamera`, page swap, admin tile, CSS (reuse `.flash-frame` sizing; result image same frame).
- [ ] **Step 4: Run** `npm run test:ghost test:api test:scare test:booking test:pickup`, `npx tsc --noEmit`, `npx eslint app components lib scripts`, `npm run build` → all pass.
- [ ] **Step 5: Browser check on `http://localhost:3000` (dev server already running) via claude-in-chrome:**
  - camera denied (block permission) → notice + mock camera visible and shooting (Review Focus 1);
  - with a camera: open → live → shoot → "กำลังล้างฟิล์ม…" ≥ 2 s → result with ghost behind the person, head uncovered; download link works; retake restarts; navigating away turns the camera off (Review Focus 4);
  - force `segmentPerson` failure (rename model URL temporarily in devtools / block request) → edge-variant photo still produced (Review Focus 2);
  - admin dashboard shows the shot count increasing; stop backend → shooting unaffected (Review Focus 5);
  - set `localStorage` figure state to mirror-armed (see `lib/scare/storage.ts` KEYS) → next live shot shows the black figure behind, then state resets.
- [ ] **Step 6: Owner checkpoint** — ask the user to try it on their phone against the dev server (camera needs https or localhost; on a phone use the deployed preview or a tunnel) and approve the look.
- [ ] **Step 7: Commit** `feat: live spirit camera with download and shot counter`.

### Task 6: Ship

- [ ] **Step 1:** Rebuild local Docker backend (`docker compose up -d --build backend`), confirm `POST /api/ghost-shots` → 204.
- [ ] **Step 2:** After owner approval, push `feat/no-ticket-numbers` in both repos and merge to `main` (backend first), per the user's instruction at that time.
