# Waiting on you

Things I skipped or decided for you while you were away. Each has what I did in the meantime.

M6, M7, M8 and M10 were signed off on 2026-11-11 and committed. M9 is partly done (its rest waits on you, item 7).

1. **Your disk was almost full.** 436 of 460 GB were used; at one point only 116 MB were free,
   and every command failed.
   *Meantime:* I deleted only my own regenerable files: my session's QA screenshots (about
   1 GB) and `target/debug` (debug build output, rebuilt automatically when needed). macOS then
   freed about 17 GB on its own (its purgeable space). It's at 18 GB free now, which is enough
   to work, but the biggest folders are elsewhere on the disk, not in this project. I also
   closed the game window I'd left open for you; it was holding memory while the machine was
   swapping.
2. **Choices I made in M6 that you might not want:**
   - **Founders already know building, farming and hunting.** Nobody has to teach them. Farming
     still has to start on its own: someone gathers wild grain, then decides to clear a field.
   - **Fields wait for the first finished home.** Without that, farming started in the first
     6 hours, before anyone had a roof.
   - **One harvest a year per field.** Growth is slow enough that a spring sowing is cut in
     autumn (early, before the frost, if it isn't quite ripe).
   - **The trip-time stat** compares the walks people make with the same walks over unworn
     ground ("trails save 32% of walking time"). Raw minutes per 100 m also fall, but they mix
     in where people happen to walk.
   - **The overhunting check** kills three quarters of the deer at once (as a heavy hunting
     season would) rather than waiting for people to overhunt. People do hunt on their own, but
     only 4 to 6 kills per colony in 6 years: never enough to overhunt.
   - **Empty shelters fall down** after 4 days, so the camp doesn't fill with ruins.
3. **Known weak spots** (not blocking; tell me if any matter now):
   - **Food runs low** at the end of every winter (0 to 3 days left).
   - **Better since M7:** over 4 seeds and 6 years, one death of cold (it was 2 starved and 3
     cold at the end of M6). Colonies reach 21 to 31 people (it was 16 to 29). People rest
     less, now that they have hobbies.
4. **Choices I made in M7 that you might not want:**
   - **Five free-time activities:** stroll, sing, garden, carve, decorate. "Lay stones" is the
     first step of decorating a home.
   - **Beauty makes people happier and less bored, and draws their strolls.** Mood doesn't drive
     choices in the sim yet, so its effect on what people do is through boredom and strolls.
   - **Rest changed:** people now rest more when tired and less when food is short. This and free
     time made colonies bigger (21 to 31 by year 6).
   - **A fix outside M7:** babies were left out in the cold when their parents slept at home (an
     M6 bug). They now sleep under the same roof.
5. **M8 used your Claude plan.** Haiku (through `claude -p`, via `tools/haiku_driver`) answered
   the bridge in three test runs:
   - 2 game hours: 4 calls;
   - 1 game day: 47 calls, about 8 s each;
   - the 3-day check: 131 calls, 20 minutes.
   Each call sends one batch of up to 24 decisions. Nothing else used it.
6. **M8 parts I skipped (they need you):**
   - **A local language model** (llama.cpp, for chat and reading your words on Medium/High). It
     needs a model download of a few GB, and you asked me to check before downloading.
     *Meantime:* chat is code templates and your words are read by keyword (the Low rung). The
     Medium and High buttons say what they're waiting for.
7. **M9 is started, not finished.** Done:
    - **the scorer:** v3 is distilled from the rules brain with rounds of its own play
      (`colony-run distill`). On 30 unseen seeds its colonies keep as many people alive as rules,
      with no deaths of hunger or cold, and get 85% of the grain and 79% of the things made
      (v1 got 35% of the grain). About 140 µs per decision.
    - `colony-run arena` (now also listing where each brain parts from rules, shown on the Arena
      page) and `colony-run fork` (branching saves).
    - **Not committed:** model weights in `brain/models` are gitignored. To rebuild v3, run
      `colony-run distill --seeds 1..12 --days 48 --rounds 3 --keep 0.4 --out
      brain/models/scorer@rules-v3.json` (about 12 minutes; it's deterministic).

    Waiting on you:
    - **The text model** trains on Colab with a T4, under your Google account.
    - **The Laya adapter** needs v1's brain server set up again, with its packages installed.
    - **A scorer trained on Haiku** needs more Haiku decisions than the ~530 we have (a dataset
      of about 10 seeds × 3 days is about 1,300 calls). Say if you want me to run that on your
      plan.
    - **A change I made:** the scorer trains in Rust instead of Python, so there's nothing to
      install. ARCHITECTURE.md section 9 says so.
8. **A finding worth a look:** Haiku colonists built nothing in 3 days; rules colonists start
    shelters on day 1. The bridge's instructions tell Haiku to put urgent needs first. If you
    want them to settle, that instruction (`crates/brains/src/bridge.rs`, `INSTRUCTIONS`) is the
    place to change.
9. **The Haiku answers are worth keeping.** 512 decisions Haiku made (with its reasons) are in
    `brain/data/haiku` (4.8 MB, not gitignored). The raw traces in `runs/` are gitignored.
    *Meantime:* left as files. Say whether to commit them with the next commit.
10. **M10 (signed off; choices I made that you might not want):**
    - **The game opens to a main menu** now, over a world going on behind it. "Play this one"
      plays that world; `--no-menu` skips the menu (tools and perf runs skip it on their own).
    - **History became "Ledgers"** in the top bar: one page with tabs for Society, Economy,
      Knowledge, Map and History (and Arena in developer view).
    - **Hunting is now a pack,** on by default and switchable per world in the new-world menu.
      Moving it changed nothing (the same colonies, event for event).
    - **Gifts are fixed amounts:** 30 berries, 30 grain, 15 meat, 20 water, 20 wood, 30 sticks,
      10 stone, or a finished shelter, house or storehouse.
    - **Saves** go in ~/Library/Application Support/Colony: five slots and an autosave (each
      in-game morning, at most every 3 minutes, and when you quit).
    - **Lower power** means no shadows, at most 60 fps, and the rules brain only.
    - **World sizes** on offer are 192, 256 and 320 m.
    - **Esc** closes whatever's open; with nothing open, it brings up the pause menu.
11. **Audio** waits, as you chose: it's in LATER.md, with what the basics would be.
12. **I closed your game window** when I started testing the menus in a driven window (it's
    open again now).
13. **The first blind playtest** is in `docs/playtests/blind-01.md` (an agent playing the real
    window for two in-game seasons). Most of what it found is fixed (ROADMAP.md, M10). What it
    wanted that I haven't built, for you to choose from:
    - names over people's heads;
    - choosing where a gift lands;
    - a god tool that meddles rather than helps (the vision puts disasters and the like later);
    - asking someone to do something for someone else;
    - the start, where all ten stand in two heaps (behaviour, not a bug).
    Also: would you like a second blind playtest after your own? It takes about 30 minutes.
