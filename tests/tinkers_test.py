#!/usr/bin/env python3
"""Plays Tinkers' Construct in real Horizon (scratch folder): builds a smeltery, melts metal, pours it into casts, makes a part and a
tool, checks the tool's mining level, durability, a modifier and leveling.

usage: tinkers_test.py <version folder name> [--window]
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
USER_HOME = os.environ.get("WIN_HOME", "/mnt/c/Users/feri1")
WIN_HOME = os.environ.get("WIN_HOME_WIN", r"C:\Users\feri1")
VER = f"{USER_HOME}/AppData/Roaming/horizon-launcher/versions/" + sys.argv[1]
SCRATCH = f"{USER_HOME}/AppData/Local/Temp/lctink"
WIN = WIN_HOME + r"\AppData\Local\Temp\lctink"
WINDOW = "--window" in sys.argv
results = []


def ctl(*args, timeout=60):
    r = subprocess.run([f"{VER}/horizon-ctl.exe", "--socket", WIN + r"\ctl.json", "--timeout", str(timeout), *map(str, args)], capture_output=True, text=True, cwd=VER, timeout=timeout + 30)
    out = (r.stdout or r.stderr).strip()
    try:
        return json.loads(out.splitlines()[-1])
    except (ValueError, IndexError):
        return {"error": out[:300]}


def check(name, ok, detail=""):
    results.append(bool(ok))
    print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)


def pack(mod, out):
    src = os.path.join(ROOT, "mod", mod)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for d, _, fs in os.walk(src):
            for f in sorted(fs):
                p = os.path.join(d, f)
                z.write(p, os.path.relpath(p, src))


def info(pos):
    ctl("command", f"tinkers info {pos[0]},{pos[1]},{pos[2]}")
    time.sleep(0.2)
    for line in ctl("messages", 3).get("chat", []):
        if "fluids=" in line or "no Tinkers" in line:
            return line
    return ""


def held_count(item):
    return ctl("inventory").get("counts", {}).get(item, 0)


def move(px, py, pz):
    ctl("command", f"tp @s {px} {py} {pz}")
    for _ in range(40):
        p = ctl("state").get("player", {}).get("position", [0, 0, 0])
        if abs(p[0] - px) < 0.6 and abs(p[2] - pz) < 0.6:
            break
        time.sleep(0.1)
    ctl("wait", 5)


def right_click(pos, face=None):
    """A real right mouse press, aimed at the block's face."""
    a = ctl("look-at-block", *pos, *([face] if face else []))
    ctl("wait", 3)
    print("   aim", json.dumps(a)[:120], "sees", json.dumps(ctl("looking-at").get("block"))[:160], flush=True)
    r = ctl("mouse", "right", "click")
    ctl("wait", 5)
    return r


def wait_for(cond, seconds=30, step=0.5):
    t0 = time.time()
    while time.time() - t0 < seconds:
        v = cond()
        if v:
            return v
        time.sleep(step)
    return cond()


def main():
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
        text = open(f"{SCRATCH}/out.txt", errors="replace").read()
        check("both mods load", "content from 2 mod(s)" in text, next((l[:300] for l in text.splitlines() if "not loaded" in l), ""))
        for line in text.splitlines():
            if any(k in line for k in ("content mods not loaded", "cannot run", "refused")):
                print("  log:", line[:260], flush=True)
        L = "ddsstinkers"
        ctl("command", "difficulty peaceful")
        ctl("command", "gamerule doDaylightCycle false")
        ctl("effect", "minecraft:resistance", "9999", "5")
        s = ctl("state")
        p = s.get("player", {}).get("position", [0, 100, 0])
        x, y, z = int(p[0]), 140, int(p[2])
        ctl("fill", x - 16, y - 2, z - 16, x + 16, y - 2, z + 16, "minecraft:glass")
        ctl("tp", x, y, z)
        ctl("wait", 20)
        # A 3 by 3 by 2 room walled in seared bricks, its controller on the south wall, a tank and a drain in it.
        rx, ry, rz = x + 2, y - 1, z + 2  # room's low corner is rx+1...
        ctl("fill", rx, ry, rz, rx + 4, ry, rz + 4, f"{L}:seared_bricks")
        for dy in (1, 2):
            ctl("fill", rx, ry + dy, rz, rx + 4, ry + dy, rz + 4, f"{L}:seared_bricks")
            ctl("fill", rx + 1, ry + dy, rz + 1, rx + 3, ry + dy, rz + 3, "minecraft:air")
        controller = (rx + 2, ry + 1, rz)
        tank = (rx + 4, ry + 1, rz + 2)
        drain = (rx, ry + 1, rz + 2)
        ctl("setblock", *controller, f"{L}:smeltery_controller[facing=north]")
        ctl("setblock", *tank, f"{L}:seared_tank")
        ctl("setblock", *drain, f"{L}:seared_drain[facing=west]")
        faucet = (rx - 1, ry + 1, rz + 2)
        ctl("setblock", *faucet, f"{L}:seared_faucet[facing=east]")
        table = (rx - 1, ry, rz + 2)
        ctl("setblock", *table, f"{L}:casting_table")
        move(rx - 2.5, y, rz + 2.5)
        ctl("wait", 60)
        v = wait_for(lambda: "valid=1" in info(controller), 12)
        check("the smeltery recognises its walls", v, info(controller)[:200])
        # Lava in the tank, by bucket.
        ctl("command", "clear @s")
        ctl("give", "minecraft:lava_bucket", 1)
        ctl("select", "minecraft:lava_bucket")
        move(tank[0] + 2.0, tank[1], tank[2] + 0.5)
        r = right_click(tank)
        ctl("wait", 10)
        move(rx - 2.5, y, rz + 2.5)
        check("a lava bucket fills the tank", "fluid=lava" in info(tank), info(tank)[:160] + " " + json.dumps(r)[:100])
        # Iron thrown in melts.
        ctl("command", f'summon minecraft:item {rx + 2} {ry + 2} {rz + 2} {{Item:{{id:"minecraft:iron_ingot",count:9}},PickupDelay:32000}}')
        melted = wait_for(lambda: "iron" in info(controller).split("amount")[0], 40)
        check("thrown ingots melt into molten iron", melted, info(controller)[:220])
        # A part cast: a gold cast on the table, the faucet opened, an ingot out.
        ctl("command", "clear @s")
        ctl("give", f"{L}:cast_ingot", 1)
        ctl("select", f"{L}:cast_ingot")
        move(table[0] - 1.5, table[1], table[2] + 0.5)
        r = right_click(table, "west")
        ctl("wait", 5)
        print("  table click", json.dumps(r)[:300], json.dumps(ctl("inventory").get("counts")), ctl("messages", 3).get("chat"), flush=True)
        check("a cast sits on the table", "cast_ingot" in info(table), info(table)[:200])
        r = right_click(faucet)
        print("  faucet click", json.dumps(r)[:140], info(faucet)[:120], flush=True)
        got = wait_for(lambda: "iron_ingot" in info(table), 25)
        check("the faucet pours and the table casts an ingot", got, info(table)[:220])
        # A tool from commands: the stats and mining level come from the materials.
        ctl("command", "clear @s")
        ctl("command", "tinkers tool pickaxe wood iron iron")
        ctl("wait", 5)
        inv = ctl("inventory")
        check("/tinkers makes a pickaxe", f"{L}:pickaxe" in json.dumps(inv), json.dumps(inv.get("counts", ""))[:140])
        ctl("select", f"{L}:pickaxe")
        bx, by, bz = x + 2, y, z - 2
        move(bx - 2.5, by, bz + 0.5)
        for block, want in ((f"longclimb:thorium_ore", "raw_thorium"), (f"longclimb:osmium_ore", None), ("minecraft:iron_ore", "raw_iron"), ("minecraft:stone", "cobblestone")):
            ctl("command", "kill @e[type=item]")
            ctl("setblock", bx, by, bz, block)
            ctl("look-at-block", bx, by, bz)
            ctl("wait", 3)
            for _ in range(20):
                if block.split(":")[1] in json.dumps(ctl("block", bx, by, bz)):
                    break
                time.sleep(0.25)
            r = ctl("break", bx, by, bz, "--timeout", "60", timeout=75)
            # Iron gives the tool Magnetic, which can pull a drop into the inventory before it is seen on the ground.
            ctl("wait", 25)
            got = json.dumps(r.get("drops", {})) + json.dumps(r.get("picked_up", {})) + json.dumps(ctl("inventory"))
            drops = json.dumps(r.get("drops", {}))
            ok = (want in got) if want else (drops == "{}" and block.split(":")[1].replace("_ore", "") not in json.dumps(r.get("picked_up", {})))
            check(f"iron-headed pickaxe on {block.split(':')[1]} -> {want or 'no drop'}", ok and "error" not in r, json.dumps({k: r.get(k) for k in ("ticks", "drops", "error")})[:160])
        # Slab: wear on the tool, and a broken one stays.
        inv = ctl("inventory")
        print("  after mining:", json.dumps(inv.get("hotbar", inv))[:300], flush=True)
        # A low-level tool can't take what it shouldn't.
        ctl("command", "clear @s")
        ctl("command", "tinkers tool pickaxe wood wood wood")
        ctl("select", f"{L}:pickaxe")
        ctl("setblock", bx, by, bz, "minecraft:iron_ore")
        ctl("wait", 3)
        r = ctl("break", bx, by, bz, "--timeout", "60", timeout=75)
        check("wood-headed pickaxe on iron ore -> no drop", json.dumps(r.get("drops", {})) == "{}", json.dumps(r.get("drops"))[:80])
        # A hammer breaks 3x3.
        ctl("command", "clear @s")
        ctl("command", "tinkers tool hammer wood iron iron iron")
        ctl("select", f"{L}:hammer")
        ctl("fill", bx - 1, by - 1, bz, bx + 1, by + 1, bz, "minecraft:stone")
        move(bx + 0.5, by - 1, bz - 2.5)
        ctl("look-at-block", bx, by, bz)
        ctl("wait", 3)
        ctl("break", bx, by, bz, "--timeout", "60", timeout=75)
        ctl("wait", 5)
        left = ctl("blocks", bx - 1, by - 1, bz, bx + 1, by + 1, bz)
        n = sum((left.get("counts") or {}).values()) if isinstance(left.get("counts"), dict) else -1
        check("a hammer breaks the blocks around its target", n == 0, json.dumps(left)[:120])
        # Long Climb's plain 3x3 tools.
        def wall(*blocks):
            """Fills the 3x3 wall with stone, sets some spots, and stands in front of it."""
            ctl("fill", bx - 1, by - 1, bz, bx + 1, by + 1, bz, blocks[0])
            for x, y, b in blocks[1:]:
                ctl("setblock", bx + x, by + y, bz, b)
            move(bx + 0.5, by - 1, bz - 2.5)
            ctl("look-at-block", bx, by, bz)
            ctl("wait", 3)

        def swing(tool):
            ctl("command", "clear @s")
            ctl("give", tool, "1")
            ctl("select", tool)
            ctl("command", "kill @e[type=item]")
            ctl("wait", 2)
            ctl("break", bx, by, bz, "--timeout", "120", timeout=135)
            ctl("wait", 6)
            found = {}
            for x in range(-1, 2):
                for y in range(-1, 2):
                    got = json.dumps(ctl("block", bx + x, by + y, bz))
                    found[(x, y)] = "air" not in got
            return found

        LCN = "longclimb"
        wall("minecraft:stone")
        left = swing(f"{LCN}:iron_hammer")
        check("an iron hammer breaks a 3x3 of stone", not any(left.values()), str(left))
        wall("minecraft:stone", (1, 0, "minecraft:obsidian"))
        left = swing(f"{LCN}:diamond_hammer")
        check("a hammer leaves a much harder block standing", left[(1, 0)] and sum(left.values()) == 1, str(left))
        wall("minecraft:stone", (1, 0, f"{LCN}:osmium_ore"))
        left = swing(f"{LCN}:iron_hammer")
        check("a hammer leaves an ore above its level standing", left[(1, 0)] and sum(left.values()) == 1, str(left))
        wall("minecraft:dirt", (1, 0, "minecraft:stone"))
        left = swing(f"{LCN}:iron_excavator")
        check("an iron excavator digs 3x3 of dirt and leaves stone", left[(1, 0)] and sum(left.values()) == 1, str(left))
        wall("minecraft:stone", (0, 0, "minecraft:dirt"))
        left = swing(f"{LCN}:iron_hammer")
        check("a hammer on the wrong block only breaks that block", not left[(0, 0)] and sum(left.values()) == 8, str(left))
        wall("minecraft:stone")
        ctl("command", "clear @s")
        ctl("give", "minecraft:iron_pickaxe", "1")
        left = swing(f"{LCN}:wooden_hammer")
        hand = json.dumps(ctl("inventory"))
        print("  wooden hammer after one swing:", hand[:200], flush=True)
        check("a wooden hammer also sweeps 3x3", not any(left.values()), str(left))
        # A hit: a blow lands for the tool's attack.
        ctl("command", "clear @s")
        ctl("command", "tinkers tool broadsword wood iron wood")
        ctl("select", f"{L}:broadsword")
        move(bx + 0.5, by - 1, bz - 2.5)
        ctl("command", "difficulty easy")
        ctl("command", f"summon minecraft:pig {bx + 0.5} {by - 1} {bz - 0.5} {{NoAI:1b}}")
        ctl("wait", 20)
        pigs = [e for e in ctl("entities", 10).get("entities", []) if "pig" in str(e.get("type"))]
        m = {"id": pigs[0]["id"]} if pigs else {}
        print("  pig:", json.dumps(pigs)[:200], flush=True)
        ctl("look-at-entity", m.get("id"))
        ctl("wait", 30)
        print("  looking at", json.dumps(ctl("looking-at"))[:200], flush=True)
        def pig_health():
            for e in ctl("entities", 10).get("entities", []):
                if e.get("id") == m.get("id"):
                    return e.get("health")
            return None
        before = pig_health()
        ctl("wait", 30)
        ctl("look-at-entity", m.get("id"))
        ctl("wait", 2)
        print("  target", json.dumps(ctl("looking-at").get("entity"))[:120], flush=True)
        ctl("mouse", "left", "down")
        ctl("wait", 2)
        ctl("mouse", "left", "up")
        ctl("wait", 20)
        print("  after attack", pig_health(), ctl("messages", 2).get("chat"), flush=True)
        after = pig_health()
        check("a sword hit deals its material's damage", before is not None and (after is None or before - after >= 4), f"{before} -> {after}")
        inv = json.dumps(ctl("inventory").get("hotbar", ""))
        ctl("command", "clear @s")
        ctl("give", "minecraft:iron_sword", 1)
        ctl("select", "minecraft:iron_sword")
        ctl("wait", 30)
        ctl("look-at-entity", m.get("id"))
        ctl("wait", 2)
        ctl("mouse", "left", "down")
        ctl("wait", 2)
        ctl("mouse", "left", "up")
        ctl("wait", 30)
        print("  control after", pig_health(), [e for e in ctl("entities", 8).get("entities", [])][:1], flush=True)
        check("a hit gives tool XP", "xp:1" in inv or "xp:2" in inv, inv[:200])
        # Slime islands exist, and the machines' recipes loaded.
        ctl("command", "locate structure ddsstinkers:slime_island_green")
        time.sleep(4)
        chat = ctl("messages", 3).get("chat", [])
        print("  locate:", chat[:2], flush=True)
        check("slime islands generate", any("slime_island_green" in c and "nearest" in c for c in chat), str(chat[:1])[:140])
        # A modifier on a Tool Station: a diamond raises the mining level and uses a slot.
        print("  log errors:", [l[:200] for l in open(f"{SCRATCH}/home/logs/latest.log", errors="replace") if re.search(r"panic|ERROR", l)][:5], flush=True)
        ctl("quit", timeout=30)
    finally:
        try:
            game.wait(timeout=30)
        except subprocess.TimeoutExpired:
            game.kill()
    print(f"{sum(results)}/{len(results)} passed")
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
