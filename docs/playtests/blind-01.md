# Blind playtest 01: Colony

| | |
|---|---|
| What I was given | The launch command, the drive-file command list (click/move/drag/scroll/key/hold/type/wait/shot/info/quit), and the one line "a god-game sandbox: a handful of AI-driven people live in a generated region, and you watch, help or meddle" |
| Contamination | **Partial. SESSION CONTAMINATED.** Before play, the harness auto-attached the project's CLAUDE.md and the git status and commit messages. From those I knew the people have "brains" that record decision traces, that Haiku-model brains are planned, and the milestone names ("families and society", "settling, leisure and beauty", "gifts", "packs", "saves", "chronicle"). I read no docs, source or data files. Conclusions this could have influenced are flagged with **[C]**. |
| Seed | 4242 (my own), default size, Beauty on, Hunting pack on |
| How I drove it | Command files into the real window, screenshots read back. This counts as playing, not reading `--dump`, but every input was a written file with a round trip of a few seconds. I could not react in real time, so I paused a lot. |
| Sessions | 3 launches (the game's screenshot output died twice and I had to relaunch). About 82 command files in all. |
| In-game time | Spring day 1 to autumn day 3 of year 1: about two seasons, 10 people growing to 12 |
| How long I lasted | To the end of the budget. The pull dropped hard twice: see question 3. |

---

## The play, in order

### First five minutes

**The main menu.** A pixel-art forest scrolls behind a small dark card: "Colony. Watch them live, help or meddle, and compare minds." The buttons are New world, Play this one (the world behind the menu), Load, Settings and Quit. Behind the card a knot of tiny people stands in the trees. I expected "compare minds" to mean nothing for an hour. It turned out to be literal (the Arena page).

**New world.** I expected a seed box and maybe a size. I got a seed, a size, a "Beauty matters" rule and a "Hunting" pack. I typed 4242 and pressed Start. It was clear and fast. I had no idea what "packs" meant beyond hunting, and nothing told me.

**First look.** It's an isometric valley with two rivers, pines and broadleaves, reeds and berry bushes, all lovely. Ten people stand in two tight overlapping heaps at the reeds by each river, like a rugby scrum. I expected them to spread out and start doing things. At 1x they stood around for a while.

**Clicking a person.** I clicked the middle of a body and the result was "selected nobody". I clicked again lower, and later got Rhea by clicking what I thought was empty ground near her. I assumed for a long time that I was bad at aiming. Much later I worked out the game was picking at about 1.2× my cursor position (see Bugs). **This is the single biggest thing that made my first hour feel bad.** Every "missed" person was this.

**The person panel (Rhea, then Knut).** This is when it clicked for me. The panel has a name, age, role ("Explorer · man · the camp"), "Now: Rest by the fire" and a spoken line ("I'll sit by the fire for a while."). Then Needs bars (Hunger, Thirst, Rest, Warmth, Social, Fun, Health), a mood total with the reasons (−3 Very tired, +2 Fed and watered, +2 Good company, +1 By the fire, +1 Lovely here) and recent thoughts. The tabs are Mind, Life (family, "close to" list, memories), Body (height, build, "DNA" with skin, hair and eyes marked "founder", and a personality of curiosity, diligence, sociability, warmth and calm), Skills and Log. I expected a Sims-like readout and got one, and it's good. Knut is curious, low on sociability, warm and low on calm. I wanted to watch whether that showed.

**"1 decisions" / "Brain: rules".** There are two pills under the name. "1 decisions" looks exactly like a button and does nothing when clicked, and has no tooltip. The Log tab had the real thing: one entry, "Rules agreed · under way · Open trace", and the trace opened a breakdown of the options considered. **[C]** I knew to expect a decision trace from CLAUDE.md, so a stranger might find this more opaque than I did. "Rules agreed" in particular means nothing without context. I read it at first as two people agreeing on rules.

### What I did, roughly in order

1. **Paused, poked the top bar.** It shows "The camp · 10 people", the date and time, weather ("Clear, 11 C, fire lit, 10 h of fuel"), pause/1x/3x/10x, Ledgers, World, Arena, "Detail: normal", **"Dev: on"** (on by default, and highlighted) and an fps counter.
2. **Ledgers.** I looked at the tabs. **Society** has households, generations, bonds, tensions and a mood bar. **Economy** has stock cards with little bar-chart histories and buildings and fields. **Knowledge** is a table of who knows what. Everyone knew nearly everything from minute one, so I couldn't see why it mattered. **Map** and **History** too. History is the best page in the game, more below.
3. **Watched the fire.** Within the first day the fire burned out and wood was at 0. Nobody seemed bothered.
4. **Speak.** I selected Knut and pressed "Speak to Knut". A box appeared: "Say to Knut (Enter to speak, Esc to stop). For example: 'fetch some wood', 'plant flowers', 'go and rest'". (The prompt and the example text overlap each other in the box: "stop)" runs into "For".) I typed *"could you bring back some wood for tomorrow"* and pressed Enter. Expectation: he says something back. What happened: nothing at all while paused. After I unpaused, the Story feed showed my line with "(fetch wood)" after it, and his panel showed in gold "Heard you: … (read as: fetch wood)". So it's keyword matching and it tells you so, which I liked. But Knut's "Now" stayed "Rest by the fire" and he never said a word about it. I never saw him fetch wood. I couldn't tell whether he'd agreed, refused, or would get round to it.
5. **Gift.** There are three columns. Food: 30 berries, 30 grain, 15 meat, 20 water. Materials: 20 wood, 30 sticks, 10 stone. Buildings: Shelter, House, Storehouse. The footer says "Gifts arrive at once and go in the story feed and the history." I gave 20 wood while paused. The tooltip still said "(they have 0)" and nothing appeared in the feed, so I thought it had failed. Only after unpausing did the feed say "You gave 20 wood in the stores". I gave a Storehouse and, after a second of game time, **a red-roofed barn popped into existence** next to camp. That was a good moment. Gifts "arrive at once" except when paused, and paused is exactly when you'd reach for them.
6. **Pause menu, save, settings.** Escape opens Paused (Resume / Save / New world / Load / Settings / Quit, "Your world is saved to the autosave first"). Save opens five slots, and clicking Slot 1 saves silently and drops you back on the Pause menu. The only confirmation is in the Story feed. Settings has Power (Normal / Lower power), Text and panels (Small/Normal/Large, applied instantly, which is nice) and Windowed / Full screen.
7. **10x, the first stretch.** This is where the game came alive. Within about one in-game day: "Saga carved a statue", "Sela started building a shelter", "Rhea got better at building (level 2)", "Dag got better at exploring (level 4)". Shelter frames went up around camp, a statue appeared on the platform, and flower beds turned up. Then "Dag and Rhea became partners", "Saga killed a deer", "A new field is ready to sow", "Rhea sowed a field". Fields with rows of sprouts appeared. By spring day 6 a proper timber-frame house with lit windows stood south of the fire.
8. **Lenses.** Needs (rings coloured by most pressing need) and Mood (rings coloured content/okay/uneasy/unhappy) are useful in principle, but the rings sit under the feet and overlap in a crowd, so I couldn't read them. **Traffic** is lovely. The ground goes grey and footpaths show in brown and orange where people have worn trails, with "0 tiles worn into trails · Trails save 1% of walking time · 263 min/100 m (at first 263)". The first line is poetry and the second is a spreadsheet. **Soil** and **Beauty** I barely looked at.
9. **Arena.** This page has bar charts comparing "rules", "scorer@rules-v1", "scorer@rules-v3-r1" and "scorer@rules-v3-r3" over "seeds 43..72, 48 days each". The measures are Alive, Born, Died, deaths of hunger/thirst/cold, Homes, Grain, Things made, Hobbies and "Agrees with rules". Then a table of "where each brain parts from rules most" ("rest → play: 4.5%"). **[C]** I understood this as comparing AI decision-makers because CLAUDE.md told me about brains and scorers. Cold, it's a developer dashboard. "scorer@rules-v3-r1" is not a name a player should ever see.
10. **World settings.** These are Beauty matters (On), the AI budget (Low / Medium / High / Max) and Packs ("Hunting · chosen when the world was made"). Hovering Medium gives *"Medium: not yet. It needs the scorer and text model (M9) and a local language model (a download). Until then it's Low."* A milestone code in a player-facing tooltip. Three of the four buttons do nothing.
11. **Zoom.** Scrolling in gets you close enough to see faces and the texture on statues, and it looks great. Scrolling out shows the whole region, with the camp a tiny cluster in a big wilderness, a lake to the south and more rivers. There are no other settlements or animals I could spot from up there.
12. **Second stretch at 10x (spring day 3 to summer day 3).** It's dusk, people asleep around the fire, and a speech bubble with a music note: "♪ Sleep, little one, the stars are near…" (someone singing a lullaby). Later "♪ Bring the grain home, bring it home…". **Those bubbles are the moment the dots became people for me.** Summer came, "Nell was born to Rhea and Dag, the first child born in the camp", and the header changed to "11 people (1 young)". Hovering the stores gives a tooltip: "Food for 2.1 days; 40 berries, 44 grain, 4 water, 0 wood". **Rhea's house** panel: lives here Rhea and Dag, asleep inside Rhea and Dag, sleeps up to 6, keeps out 85% of the cold and all the rain, finished spring day 6.
13. **Society, summer day 3.** Seven homes, two couples, and "No home yet (they sleep by the fire): Hugo, Knut". Sten's own shelter is empty because he moved in with Sela. The mood bar showed "8 content · 1 so-so · 2 low". I went looking for the two unhappy people with the Mood lens and found only green rings, which is how I eventually discovered the click offset.
14. **Speaking to Saga.** I typed *"Hugo and Knut have nowhere to sleep, please help them"*. It was read as **"sleep"**. Saga's "Now" stayed "Carve a statue". So I told her to go to bed, and she ignored that too. That's when I stopped trusting Speak as a tool and started treating it as a novelty.
15. **Third stretch (summer day 3 to autumn day 3).** Screenshots silently died partway (see Bugs), so I quit and continued. Autumn is the second great moment. The broadleaves have turned orange, rust and red, there are three timber houses plus a new frame at 28%, two bare harvested fields, and most people indoors at 02:40 with one asleep by the fire. History had filled in what I missed: Sela and Sten partnered, then Saga and Knut, then Oona and Hugo (so my two homeless men sorted themselves out without me), "Ada was born to Sela and Sten", and "Tova brought in the first harvest: 83 grain". Economy: food for about 7 days for 12 people, grain 283.
16. **Load.** From the Pause menu I loaded Slot 1 (spring day 1). There was no "you'll lose your current world" prompt. It loaded instantly and unpaused the game. I quit, relaunched, and the main menu said "Autosave: Year 1, spring day 1 · 10 people". **The autumn world is gone.** Two seasons, two babies and three marriages, overwritten by the autosave-on-quit.

---

### 1. What did you think you were supposed to do, in your own words?

There's a little band of settlers in a valley who look after themselves: they build huts, then houses, farm, hunt, pair off and have kids. You're a sort of kindly ghost who can drop supplies on them out of the sky, or whisper suggestions they mostly don't act on. The real "game" is opening their files and reading about them, and watching the village grow at fast-forward. There's no goal and nothing to lose. I spent the whole time wondering whether I was supposed to be keeping them alive, and the honest answer seemed to be "they're fine without you".

### 2. What did you reach for that wasn't there?

- **A reply.** When I spoke to someone I wanted them to answer, even "Fine, after I rest" or "No." The game shows how it parsed my words but never what the person thought of them. I couldn't tell whether an instruction was accepted, queued or ignored, and no follow-up appeared in the feed when it was done.
- **Speaking about other people.** "Help Hugo", "build Knut a house", "make up with Sela". The keyword reader collapses all of these to one verb for the person you're speaking to.
- **Clicking a name to go to that person.** Every gold name in the ledgers, the Story feed, the house panel ("Lives here: Rhea, Dag") and the Life tab looks like a link, and none of them are. I wanted to click "Nell" and have the camera fly to the baby.
- **Following someone.** A "follow" or "centre on" button in the person panel. With a 1.2× click offset and people moving at 10x, keeping track of one person was hopeless.
- **Names over heads** (or on hover). I knew Saga from her panel but couldn't find her again in a crowd.
- **Finding the unhappy people.** Society said "2 low". I wanted to click that and get the two names. The Mood lens rings were all green from where I sat.
- **Choosing where a gift lands.** The storehouse appears wherever it likes. I wanted to place it.
- **Knowing what "packs" are**, and whether there are others.
- **Some pressure or a question to answer.** The fire went out on day 1, wood hit 0, and Story kept saying "2 people are very thirsty" with a river ten metres away, but nothing bad ever happened. I wanted to know whether these warnings mattered.
- **A way to keep the world when loading an older save**, or at least a warning.
- **Meddling.** The pitch says "help or meddle", but both god tools are help. There's no weather, no wolves, no rumours, nothing mischievous.

### 3. Where did you stop caring, and what were you looking at?

**First dip:** the Ledgers, Knowledge tab, about ten minutes in. It's a grid of who knows what, nearly all filled in on day 1, with nothing I could do about it. Right after that came the Arena page, a scoreboard of "scorer@rules-v3-r1" against "rules" over seeds 43..72. That's a page for the person building the game, not for me. It pulled me out of the valley and into a dev tool.

**Second dip, the bigger one:** around summer day 3, staring at the Mood lens and trying to click the two unhappy people. I had a goal for once ("find who's sad, cheer them up") and the game wouldn't let me select anyone. Then I spoke to Saga about the homeless men and she was told to sleep. That's when I gave up on being the god and went back to just watching at 10x.

What brought me back both times was fast-forward: houses rising, the lullaby bubble, autumn leaves.

### 4. Name one thing you now know that you were never told.

That there's an unreleased smarter AI and this build runs the dumb one. The World panel lists four AI budget levels with three disabled, and the tooltip says it needs "the scorer and text model (M9)". The Arena tab names four brain versions and scores them. A player learns the project's roadmap (milestone 9, a scorer, a download) from a tooltip. **[C]** I already half-knew this from CLAUDE.md, but the game would have told me anyway.

A second leak: the Knowledge ledger and the "Known to: 11 of 11" line on a tree panel told me that knowledge of places is tracked per person, and that everyone already knows everything. I never had to discover a single place.

---

## Bugs and broken things

1. **World clicks land at about 1.2× the cursor position.** *Did:* clicked squarely on Saga at window (904,621). *Expected:* Saga selected. *Got:* a broadleaf tree selected, its ring far to the lower right (around 1086,830). Clicking at (741,509), roughly 904/1.22 and 621/1.22, selected Saga. Earlier I got Knut by clicking well above and to the left of him. UI buttons hit correctly; only world picking (and probably world hover tooltips) is scaled. It may be caused by how the drive harness injects coordinates rather than by the game itself, but it made the first hour feel broken. Shots: `blind-shots/078.png` (tree selected when aiming at Saga), `079.png` (Saga selected with the corrected click).
2. **Clicks on empty parts of the info panel go through to the world.** *Did:* clicked a blank area of Saga's panel. *Expected:* nothing. *Got:* a tree behind or off-screen to the right was selected and the panel switched to "Broadleaf tree". `080.png`, `081.png`.
3. **Loading an older save then quitting destroys the current world.** *Did:* in autumn year 1, Pause, Load, Slot 1 (spring day 1), then Quit. *Expected:* a warning, or the autumn world kept somewhere. *Got:* no prompt, an instant load, and the autosave-on-quit overwrote the autumn autosave. Two seasons of play were lost. `095.png` (load list), `096.png` (loaded), `097.png` (menu now says Autosave: spring day 1).
4. **The Story feed is wiped on every Continue or Load.** After a load the feed shows only "Loaded: …". History keeps everything, but the feed loses its context. `091.png`, `096.png`.
5. **Load unpauses the game,** and Continue resets the speed to 1x. I was paused when I loaded and came back to a running game. Minor, but surprising.
6. **Commands given while paused silently wait for time.** Speak and Gift do nothing visible until unpaused: the store tooltip still says "(they have 0)", there's no feed line, and the panel doesn't change. The Gift footer promises "Gifts arrive at once". *Expected:* the gift lands immediately, or a "queued, will happen when time runs" note. `049.png`, `050.png` (paused, still 0), `051.png` (unpaused, 20).
7. **The `shot` command silently stopped working, twice.** No file and no result line in the done file; every other command kept working and the sim kept running. Display sleep is off on this machine, so it isn't the screen sleeping. The first time was after pressing Speak with nobody selected, while paused. The second was partway through a four-minute 10x run. Quitting and relaunching fixed it. This is in the harness rather than the game, but it ended two sessions.
8. **"1 decisions" / "11 decisions" pill** looks like a button, does nothing when clicked, and has no tooltip. It's also bad grammar at 1. (`009.png`/`010.png`, early session; the images are no longer in front of me, so this is from my notes.)
9. **Gold names everywhere look clickable and aren't** (ledgers, Life tab, house panel, Story). `073.png`.
10. **Text slips:** "You gave 20 wood **in** the stores" (should be "to"), "enjoying it (1 **times**)", "1 **decisions**". In the Speak prompt box, the prompt text and the example text overlap: "Esc to stop)" runs into "For example…" (`045.png`).
11. **The Gift tooltip covers the Storehouse button** while you hover 20 wood, so you can't see the button you're about to reach for. `049.png`.
12. **Spoken commands seem to have no effect.** Knut was told "fetch wood" and kept "Rest by the fire". Saga was told "sleep" (misparsed) and kept "Carve a statue". Maybe both were obeyed later, but nothing on screen ever said so. `047.png`, `083.png`.
13. **The Mood and Needs lens rings are unreadable in a crowd.** They're drawn under the feet, overlap each other, and pale green and white are hard to tell apart. Society said 2 people were low and I couldn't find them. `075.png`.
14. **The start is a scrum.** All ten people overlap in two heaps on top of each other at the reeds (`007.png`). Later, everyone bunches at the fire and their sprites overlap so badly that clicking one is a lottery.
15. **Repeated "2 people are very thirsty"** for days, with rivers on two sides of camp. It isn't necessarily a bug, but it read as broken.
16. **Dev surfaces exposed to the player:** "Dev: on" is lit by default in the top bar, there's an fps counter, the Arena tab, the "(M9)" tooltip, and brain version strings.
17. **The hover tooltip appeared away from the cursor** (the "Broadleaf tree" tooltip in `075.png`). That's consistent with bug 1.

## Confusing things

- "Rules agreed · under way" in the Log, and "Brain: rules". What rules, and agreed by whom? **[C]**
- "Packs: Hunting". What's a pack, and is there a list?
- The Knowledge tab. What am I supposed to do with it?
- "Trails save 1% of walking time · 263 min/100 m (at first 263)". Is 263 minutes per 100 m real? It sounds absurd.
- Whether anything I did mattered. The camp got houses, babies and harvests whether or not I touched it.
- "Detail: normal" in the top bar. History says "Showing the main events (Detail: normal)", so it seems to filter the chronicle, but I only learned that from History.
- The difference between Shelter and House as gifts, and why the first house was "Rhea's house" when the Society page called other homes "Sela's shelter".

## What was satisfying or delightful

- **The art.** The isometric pixel style, soft shadows, day and night, the fire glowing orange on the grass at dusk, lit windows in the houses, and **autumn turning the broadleaves orange and red**. It's a place, not terrain.
- **The singing bubbles** ("♪ Sleep, little one, the stars are near…"). That's the point at which these read as people.
- **History.** "Saga killed the camp's first deer", "Nell was born to Rhea and Dag, the first child born in the camp", "Tova brought in the first harvest: 83 grain", "A gift from the sky: a storehouse". It's a chronicle with firsts, and I'd read it like a diary. That's the best screen in the game.
- **The person panel's mood breakdown** ("+1 Lovely here", "+1 By the fire") and the personality sliders.
- **The Storehouse gift** appearing out of nowhere: instant, visible, satisfying.
- **The Traffic lens**, with "Footsteps worn into the ground" and desire paths forming between fire, stores and fields.
- **The settlement growing at 10x.** You go from a scrum of strangers to a hamlet of three houses with fields and statues in about seven minutes of real time.
- Large text applying instantly, and the five clearly labelled save slots.

## What I wanted to do and couldn't

Click a name to find that person. Follow someone with the camera. Hear a reply when I speak. Ask someone to do something for someone else. Find the two unhappy people. Place a gift where I wanted it. Do something mischievous: the pitch says "meddle" and I only had help. Load an old save without losing my current world. Watch the fire-out and thirst warnings turn into consequences, or learn that they never will.

## Would I have kept playing?

For a while, yes, but as a screensaver with a diary, not as a game. I'd come back for the moment autumn turned the leaves and for the History page. But I'd have quit as a *god* at about the 25-minute mark, when I tried to find the sad people, couldn't select anyone, and spoke to Saga only to have it read as "sleep". After that I only pressed 10x and read. And if I'd lost two seasons to the Load trap on my own machine, I'd have closed it that evening and not come back for a week. The valley and its people are already worth an evening to watch. The tools for touching them aren't there yet.

*Caveat: I'm an approximation of a stranger. Some of the "I understood the brains and the Arena" conclusions (marked [C]) depend on my having seen the project's CLAUDE.md. A real stranger would likely find those pages more baffling than I did, not less.*
