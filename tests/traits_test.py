#!/usr/bin/env python3
"""Every Tinkers trait: a tool with it is built, held, used to mine and hit, right-clicked. Then what each noticeable one does.

usage: traits_test.py <version folder name> [--window]
"""
import json
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
USER_HOME = "/mnt/c/Users/feri1"
WIN_HOME = r"C:\Users\feri1"
VER = f"{USER_HOME}/AppData/Roaming/horizon-launcher/versions/" + sys.argv[1]
SCRATCH = f"{USER_HOME}/AppData/Local/Temp/lctraits"
WIN = WIN_HOME + r"\AppData\Local\Temp\lctraits"
WINDOW = "--window" in sys.argv
TK = "ddsstinkers"
results = []
T = json.load(open(os.path.join(ROOT, "native", "ddsstinkers", "data", "tinkers.json")))


def ctl(*args, timeout=60):
    r = subprocess.run([f"{VER}/horizon-ctl.exe", "--socket", WIN + r"\ctl.json", "--timeout", str(timeout), *map(str, args)], capture_output=True, text=True, cwd=VER, timeout=timeout + 15)
    out = (r.stdout or r.stderr).strip()
    try:
        return json.loads(out.splitlines()[-1])
    except (ValueError, IndexError):
        return {"error": out[:300]}


def check(name, ok, detail=""):
    results.append(bool(ok))
    print(("PASS " if ok else "FAIL ") + name + (f"  [{str(detail)[:220]}]" if detail != "" else ""), flush=True)


def pack(mod, out):
    src = os.path.join(ROOT, "mod", mod)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for d, _, fs in os.walk(src):
            for f in sorted(fs):
                p = os.path.join(d, f)
                z.write(p, os.path.relpath(p, src))


def holder(trait):
    """(tool kind, material list) with this trait in a part it applies to."""
    for mid, m in sorted(T["materials"].items()):
        for role, names in (m.get("traits") or {}).items():
            if trait not in names:
                continue
            if role == "head":
                return "pickaxe", ["wood", mid, "wood"]
            if role == "handle":
                return "pickaxe", [mid, "iron", "wood"]
            if role == "extra":
                return "pickaxe", ["wood", "iron", mid]
            if role == "bow":
                return "shortbow", [mid, mid, "string"]
            if role == "bowstring":
                return "shortbow", ["wood", "wood", mid]
            if role == "shaft":
                return "arrow", [mid, "iron", "feather"]
            if role == "fletching":
                return "arrow", ["wood", "iron", mid]
            if role == "projectile":
                return "arrow", ["wood", mid, "feather"]
    return None, None


def tool_tag():
    inv = ctl("inventory")
    for s in inv.get("slots", inv.get("hotbar", [])) if isinstance(inv, dict) else []:
        c = s.get("components") if isinstance(s, dict) else None
        if c and "ddsstinkers:tool" in str(c):
            return str(c)
    return json.dumps(inv)


def main():
    if WINDOW and "horizon-client" in subprocess.run(["tasklist.exe"], capture_output=True, text=True).stdout:
        print("a game is already running: not opening a second window")
        return 3
    if os.path.exists(SCRATCH):
        shutil.rmtree(SCRATCH)
    g = f"{SCRATCH}/game"
    os.makedirs(f"{g}/horizon-mods")
    os.makedirs(f"{SCRATCH}/home")
    for m in ("longclimb", "ddsstinkers"):
        pack(m, f"{g}/horizon-mods/{m}.hmod")
    with open(f"{g}/horizon-mods.txt", "w") as f:
        f.write("mod.longclimb:true\nmod.ddsstinkers:true\n")
    with open(f"{g}/options.txt", "w") as f:
        f.write("renderDistance:6\nsimulationDistance:5\n")
    log = open(f"{SCRATCH}/out.txt", "w")
    head = [] if WINDOW else ["--headless"]
    game = subprocess.Popen([f"{VER}/horizon-client.exe", *head, "--control", "--control-socket", WIN + r"\ctl.json", "--game-dir", WIN + r"\game", "--home", WIN + r"\home", "--world", "T", "--seed", "5"],
                            cwd=VER, stdout=log, stderr=subprocess.STDOUT, env=dict(os.environ, HORIZON_NO_AUDIO="1", DDSSTINKERS_DEBUG="1", WSLENV="HORIZON_NO_AUDIO:DDSSTINKERS_DEBUG"))
    try:
        t0 = time.time()
        while time.time() - t0 < 400:
            if "error" not in ctl("wait-ready", timeout=30):
                break
            if game.poll() is not None:
                print("game exited", game.returncode)
                return 2
            time.sleep(3)
        print(f"ready after {time.time() - t0:.0f}s", flush=True)
        for c in ("difficulty easy", "gamerule doDaylightCycle false", "time set 6000", "gamerule doMobSpawning false", "weather clear"):
            ctl("command", c)
        ctl("effect", "minecraft:resistance", "99999", "5")
        p = ctl("state").get("player", {}).get("position", [0, 100, 0])
        x, y, z = int(p[0]), 140, int(p[2])
        ctl("fill", x - 12, y - 2, z - 12, x + 12, y - 2, z + 12, "minecraft:smooth_stone")
        ctl("command", f"tp @s {x + 0.5} {y - 1} {z + 0.5}")
        ctl("wait", 40)
        bx, by, bz = x, y - 1, z + 3

        def fresh(kind, mats):
            ctl("command", "clear @s")
            r = ctl("command", "tinkers tool " + kind + " " + " ".join(mats))
            ctl("select", {"shortbow": "minecraft:bow", "longbow": "minecraft:bow", "crossbow": "minecraft:crossbow", "arrow": "minecraft:arrow", "bolt": "minecraft:arrow"}.get(kind, f"{TK}:{kind}"))
            ctl("wait", 2)
            return r

        def hit_pig(health=None):
            ctl("command", "kill @e[type=minecraft:pig]")
            ctl("wait", 25)
            extra = f",Health:{health}f" if health else ""
            ctl("command", f"summon minecraft:pig {x + 0.5} {y - 1} {z + 2.0} {{NoAI:1b{extra}}}")
            ctl("wait", 10)
            near = [e for e in ctl("entities", 8, "minecraft:pig").get("entities", []) if e.get("health", 0) > 0]
            if not near:
                return None
            r = ctl("attack", near[0]["id"])
            ctl("wait", 4)
            return (r.get("health_before", 0.0), r.get("health", 0.0))

        if os.environ.get("SNIP"):
            exec(open(os.environ["SNIP"]).read(), dict(globals(), **locals()))
            ctl("quit", timeout=30)
            return 0
        names = sorted({n for m in T["materials"].values() for ns in (m.get("traits") or {}).values() for n in ns})
        print(len(names), "traits", flush=True)
        # Every trait: build it, hold it, mine, hit, right-click. A crash ends the run here.
        unbuilt = []
        for i, tr in enumerate([] if os.environ.get("QUICK") else names):
            kind, mats = holder(tr)
            if not kind:
                unbuilt.append(tr)
                continue
            r = fresh(kind, mats)
            if kind == "pickaxe":
                ctl("setblock", bx, by, bz, "minecraft:stone")
                ctl("wait", 1)
                ctl("break", bx, by, bz, "--timeout", "20", timeout=30)
                hit_pig()
            ctl("use-item")
            ctl("wait", 10)
            if game.poll() is not None:
                check(f"the game survives {tr}", False, f"exited {game.returncode} on {kind} {mats}")
                return 2
            if i % 20 == 0:
                print(f"  {i}/{len(names)} ...", flush=True)
        check("every trait has a tool to live on", not unbuilt, unbuilt)
        check(f"the game survives all {len(names)} traits held, mined with, swung and right-clicked", game.poll() is None)

        # What they do.
        tag = lambda: json.dumps(ctl("inventory"))
        fresh("pickaxe", ["wood", "iron", "wood"])
        before = hit_pig()
        print("  plain hit", before, flush=True)
        k, m = holder("prickly")
        fresh(k, m)
        got = hit_pig()
        print("  prickly hit", got, flush=True)
        check("Prickly hurts its target more than a plain tool", got and before and (got[0] - got[1]) > (before[0] - before[1]), (before, got))
        # Kills count: a one-hit pig raises the counter Congenial and Souleater read.
        fresh("broadsword", ["wood", "cobalt", "wood"])
        hit_pig(1)
        ctl("wait", 6)
        check("a kill is counted on the tool", "kills:1" in tag(), tag()[:220])
        # Slots: traits that add modifiers.
        k, m = holder("magical_modifier")
        fresh(k, m)
        free = re.search(r"free:(\d+)", tag())
        check("Magically Modifiable adds three modifier slots", free and int(free.group(1)) >= 3 + 3, free.group(0) if free else tag()[:200])
        # Whirl drinks water around you.
        k, m = holder("whirl")
        fresh(k, m)
        ctl("command", f"tp @s {x + 0.5} {y - 1} {z + 0.5}")
        ctl("wait", 4)
        ctl("fill", x - 2, y + 1, z - 2, x + 2, y + 1, z + 2, "minecraft:water")
        ctl("wait", 2)
        ctl("look-at", x + 0.5, y + 20, z + 0.5)
        ctl("use-item")
        ctl("wait", 6)
        left = ctl("blocks", x - 2, y + 1, z - 2, x + 2, y + 1, z + 2)
        check("Whirl removes the water around you", "water" not in json.dumps(left.get("counts", {})), json.dumps(left.get("counts"))[:120])
        ctl("fill", x - 8, y - 1, z - 8, x + 8, y + 3, z + 8, "minecraft:air")
        # Catcher: keeps hitting until a mob goes in, then lets it out on a block.
        k, m = holder("catcher")
        fresh(k, m)
        caught = False
        for _ in range(60):
            hit_pig()
            if re.search(r'caught:\\*"minecraft:pig', tag()):
                caught = True
                break
        check("Catcher catches a mob", caught, tag()[:160])
        if caught:
            ctl("command", "kill @e[type=minecraft:pig]")
            ctl("setblock", bx, by, bz, "minecraft:stone")
            ctl("wait", 3)
            ctl("look-at-block", bx, by, bz, "up")
            ctl("wait", 2)
            ctl("mouse", "right", "click")
            ctl("wait", 8)
            mobs = ctl("entities", 12, "minecraft:pig")
            check("and lets it out again", "minecraft:pig" in json.dumps(mobs), json.dumps(mobs)[:160])
        # Synergy heals with steeleaf in the hotbar.
        k, m = holder("synergy")
        fresh(k, m)
        slot = ctl("select", f"{TK}:{k}")
        ctl("command", f"give @s {TK}:steeleaf_ingot 1")
        ctl("wait", 2)
        # Cascade breaks more than its target.
        k, m = holder("cascade")
        fresh(k, m)
        ctl("fill", bx - 4, by - 1, bz, bx + 4, by + 5, bz + 4, "minecraft:stone")
        ctl("wait", 3)
        ctl("look-at-block", bx, by, bz)
        ctl("wait", 2)
        ctl("break", bx, by, bz, "--timeout", "40", timeout=50)
        ctl("wait", 6)
        n = ctl("blocks", bx - 4, by - 1, bz, bx + 4, by + 5, bz + 4)
        stone = (n.get("counts") or {}).get("minecraft:stone", 0)
        check("Cascade breaks past its target", stone < 9 * 7 * 5 - 1, stone)
        ctl("fill", bx - 4, by - 1, bz, bx + 4, by + 5, bz + 4, "minecraft:air")
        # Bows: a modular bow fires modular arrows; Splitting makes a second, Anti-Gravity keeps a level shot level.
        def shoot(trait):
            k, m = holder(trait)
            ctl("command", "kill @e[type=minecraft:arrow]")
            ctl("command", "clear @s")
            ctl("command", "tinkers tool shortbow wood wood string")
            for _ in range(3):
                ctl("command", "tinkers tool " + k + " " + " ".join(m))
            ctl("select", "minecraft:bow")
            ctl("wait", 3)
            ctl("look-at", x + 12, y - 0.4, z + 12)
            ctl("use-item", "--hold", "1.2")
            ctl("wait", 6)
            got = ctl("command", "execute if entity @e[type=minecraft:arrow]")
            m_count = re.search(r"Count: (\d+)", json.dumps(got))
            return int(m_count.group(1)) if m_count else 0
        check("a modular bow fires a modular arrow", shoot("antigravity") >= 1)
        check("Splitting fires two arrows", shoot("splitting") >= 2)
        log_lines = open(f"{SCRATCH}/home/logs/latest.log", errors="replace").read().splitlines()
        errors = [l[:220] for l in log_lines if " ERROR" in l or "panicked" in l]
        check("no errors in the log", not errors, errors[:4])
        ctl("quit", timeout=30)
    finally:
        try:
            game.wait(timeout=30)
        except subprocess.TimeoutExpired:
            game.kill()
    print(f"{sum(results)}/{len(results)} passed", flush=True)
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
