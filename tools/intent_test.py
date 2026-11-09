"""Check the Watcher-request classifier (/intent) on tricky messages. Needs the brain server running.
   python3 tools/intent_test.py"""
import json
import sys
import urllib.request

CASES = [
    ("go chop some wood", "gather_wood"), ("can you fetch water", "fetch_water"),
    ("please build a house", "build"), ("warm up by the fire", "warm_up"),
    ("could you pick some berries for us", "gather_food"), ("go mine some stone", "gather_stone"),
    ("we need food, go work the farm", "farm"), ("take a break, you look tired", "rest"),
    ("stop farming and get water", "fetch_water"), ("check the stores for me", "check_stores"),
    ("don't chop wood", "none"), ("please don't fetch water anymore", "none"),
    ("did you fetch water?", "none"), ("never mind the well", "none"), ("well, I am tired", "none"),
    ("thanks for chopping wood yesterday", "none"), ("I love the smell of wood smoke", "none"),
    ("you should rest, no need to farm today", "rest"), ("check on the kids", "none"),
    ("hello there, how are you?", "none"), ("what do you think of the river?", "none"),
]

url = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8765/intent"
bad = 0
for msg, want in CASES:
    req = urllib.request.Request(url, json.dumps({"colonist": "Zed", "message": msg}).encode(),
                                 {"Content-Type": "application/json"})
    got = json.loads(urllib.request.urlopen(req, timeout=30).read())["request"]
    ok = got == want
    bad += not ok
    print(f"{'ok ' if ok else 'BAD'} {msg!r:45} -> {got} (want {want})")
print(f"{len(CASES) - bad}/{len(CASES)} correct")
sys.exit(1 if bad else 0)
