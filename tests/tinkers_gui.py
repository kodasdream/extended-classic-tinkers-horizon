#!/usr/bin/env python3
"""Opens Tinkers' screens in a real window, uses them with real clicks, and takes screenshots.

usage: tinkers_gui.py <version folder name>
"""
import json
import os
import shutil
import subprocess
import sys
import time
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
USER_HOME = os.environ.get("WIN_HOME", "/mnt/c/Users/feri1")
WIN_HOME = os.environ.get("WIN_HOME_WIN", r"C:\Users\feri1")
VER = f"{USER_HOME}/AppData/Roaming/horizon-launcher/versions/" + sys.argv[1]
SCRATCH = f"{USER_HOME}/AppData/Local/Temp/lctgui"
WIN = WIN_HOME + r"\AppData\Local\Temp\lctgui"
L = "ddsstinkers"


def ctl(*args, timeout=60):
    r = subprocess.run([f"{VER}/horizon-ctl.exe", "--socket", WIN + r"\ctl.json", "--timeout", str(timeout), *map(str, args)], capture_output=True, text=True, cwd=VER, timeout=timeout + 30)
    out = (r.stdout or r.stderr).strip()
    try:
        return json.loads(out.splitlines()[-1])
    except (ValueError, IndexError):
        return {"error": out[:300]}


def shot(name):
    ctl("wait", 3)
    print("shot", name, json.dumps(ctl("screenshot", WIN + rf"\shots\{name}.png", timeout=30))[:60], flush=True)


def pack(mod, out):
    src = os.path.join(ROOT, "mod", mod)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for d, _, fs in os.walk(src):
            for f in sorted(fs):
                p = os.path.join(d, f)
                z.write(p, os.path.relpath(p, src))


def move(px, py, pz):
    ctl("command", f"tp @s {px} {py} {pz}")
    for _ in range(40):
        p = ctl("state").get("player", {}).get("position", [0, 0, 0])
        if abs(p[0] - px) < 0.6 and abs(p[2] - pz) < 0.6:
            break
        time.sleep(0.1)
    ctl("wait", 5)


def open_block(pos):
    a = ctl("interact", *pos)
    time.sleep(0.8)
    return a


def slots():
    return ctl("menu")


def gui_size():
    """The game's GUI size in its own pixels, from the window and the scale."""
    w = ctl("screenshot", WIN + r"\shots\_size.png", timeout=30)
    width, height = w.get("width", 2240), w.get("height", 1260)
    scale = ctl("options-get", "guiScale").get("guiScale", {})
    scale = scale.get("value", scale) if isinstance(scale, dict) else scale
    scale = int(scale) if isinstance(scale, (int, float)) and scale else 3
    return width / scale, height / scale


results = []


def main():
    if os.path.exists(SCRATCH):
        shutil.rmtree(SCRATCH)
    g = f"{SCRATCH}/game"
    os.makedirs(f"{g}/horizon-mods")
    os.makedirs(f"{SCRATCH}/home")
    os.makedirs(f"{SCRATCH}/shots")
    for m in ("longclimb", "ddsstinkers"):
        pack(m, f"{g}/horizon-mods/{m}.hmod")
    with open(f"{g}/horizon-mods.txt", "w") as f:
        f.write("mod.longclimb:true\nmod.ddsstinkers:true\n")
    with open(f"{g}/options.txt", "w") as f:
        f.write("renderDistance:6\nsimulationDistance:5\nguiScale:3\n")
    log = open(f"{SCRATCH}/out.txt", "w")
    game = subprocess.Popen([f"{VER}/horizon-client.exe", "--control", "--control-socket", WIN + r"\ctl.json", "--game-dir", WIN + r"\game", "--home", WIN + r"\home", "--world", "G", "--seed", "5"],
                            cwd=VER, stdout=log, stderr=subprocess.STDOUT, env=dict(os.environ, HORIZON_NO_AUDIO="1", WSLENV="HORIZON_NO_AUDIO"))
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
        ctl("command", "difficulty peaceful")
        ctl("command", "gamerule doDaylightCycle false")
        ctl("command", "time set 6000")
        ctl("effect", "minecraft:resistance", "9999", "5")
        p = ctl("state").get("player", {}).get("position", [0, 100, 0])
        x, y, z = int(p[0]), 140, int(p[2])
        ctl("fill", x - 16, y - 2, z - 16, x + 16, y - 2, z + 16, "minecraft:smooth_stone")
        move(x + 0.5, y - 1, z + 0.5)
        ctl("wait", 40)
        # A row of tables, and a smeltery with fluid to look at.
        tables = {}
        for i, t in enumerate(("stencil_table", "part_builder", "tool_station", "tool_forge", "casting_table", "casting_basin", "seared_tank")):
            pos = (x - 6 + i * 2, y - 1, z + 4)
            ctl("setblock", *pos, f"{L}:{t}")
            tables[t] = pos
        # Stencil table.
        ctl("command", "clear @s")
        ctl("give", f"{L}:pattern", 8)
        ctl("give", "minecraft:oak_planks", 32)
        move(tables["stencil_table"][0] + 0.5, y - 1, z + 2.5)
        a = open_block(tables["stencil_table"])
        print("open stencil", json.dumps(a)[:100], json.dumps(ctl("screen"))[:300], flush=True)
        m = slots()
        print("menu", json.dumps(m)[:600], flush=True)
        shot("stencil")
        scr = ctl("screen")
        print("screen", {k: scr.get(k) for k in scr if k not in ("slots", "buttons", "slot_ids")}, flush=True)
        st = ctl("state")
        print("window", json.dumps(st.get("window", st.get("gui", "")))[:200], flush=True)
        # Real clicks: blank patterns into the first slot, press the tool rod's button, take the pattern out.
        print("pick", json.dumps(ctl("click-slot", 0))[:120], flush=True)
        print("put", json.dumps(ctl("click-slot", 300))[:120], flush=True)
        time.sleep(0.3)
        gw, gh = gui_size()
        ox, oy = (gw - 240) / 2, (gh - 200) / 2
        parts = sorted(json.load(open(os.path.join(ROOT, "native", "ddsstinkers", "data", "tinkers.json")))["parts"])
        i = parts.index("tool_rod")
        print("press", json.dumps(ctl("click-at", ox + 40 + (i % 10) * 18 + 8, oy + 18 + (i // 10) * 18 + 8))[:120], flush=True)
        ctl("wait", 5)
        print("take", json.dumps(ctl("click-slot", 301, "shift"))[:120], flush=True)
        ctl("wait", 5)
        counts = ctl("inventory").get("counts", {})
        print("after stencil", counts, flush=True)
        results.append(("stencil table makes a tool rod pattern", counts.get(f"{L}:pattern_tool_rod", 0) >= 1))
        ctl("click-slot", 300, "shift")
        ctl("close")
        # Part Builder: pattern and planks, press Craft, take the rod.
        move(tables["part_builder"][0] + 0.5, y - 1, z + 2.5)
        open_block(tables["part_builder"])
        ox, oy = (gw - 176) / 2, (gh - 166) / 2
        slot = lambda item: next((k for k, v in enumerate(ctl("inventory").get("hotbar", [])) if v and v.get("item") == item), None)
        ctl("click-slot", slot(f"{L}:pattern_tool_rod"))
        ctl("click-slot", 300)
        ctl("click-slot", slot("minecraft:oak_planks"))
        ctl("click-slot", 301)
        time.sleep(0.2)
        print("craft", json.dumps(ctl("click-at", ox + 100, oy + 42))[:120], flush=True)
        ctl("wait", 5)
        shot("part_builder_done")
        ctl("click-slot", 302, "shift")
        ctl("wait", 3)
        inv = ctl("inventory")
        ok = "tool_rod" in json.dumps(inv.get("counts", {})).replace("pattern_tool_rod", "") and "Wood" in json.dumps(inv)
        print("after part builder", inv.get("counts"), flush=True)
        results.append(("part builder makes a wood tool rod", ok))
        ctl("click-slot", 300, "shift")
        ctl("click-slot", 301, "shift")
        ctl("close")
        ctl("command", "tinkers part pick_head iron")
        ctl("command", "tinkers part binding cobalt")
        ctl("wait", 5)
        # Tool Station: pick the pickaxe, lay the parts, press Go, take the tool.
        move(tables["tool_station"][0] + 0.5, y - 1, z + 2.5)
        open_block(tables["tool_station"])
        tools = sorted(json.load(open(os.path.join(ROOT, "native", "ddsstinkers", "data", "tinkers.json")))["tools"])
        small = [k for k in tools if not json.load(open(os.path.join(ROOT, "native", "ddsstinkers", "data", "tinkers.json")))["tools"][k]["big"]]
        ox, oy = (gw - 240) / 2, (gh - 200) / 2
        i = small.index("pickaxe")
        ctl("click-at", ox + 8 + (i % 5) * 18 + 8, oy + 18 + (i // 5) * 18 + 8)
        time.sleep(0.4)
        for part, target in (("tool_rod", 301), ("pick_head", 302), ("binding", 303)):
            sl = next((k for k, v in enumerate(ctl("inventory").get("hotbar", [])) if v and v.get("item") == f"{L}:{part}"), None)
            print("part", part, sl, flush=True)
            if sl is None:
                continue
            ctl("click-slot", sl)
            ctl("click-slot", target)
        time.sleep(0.3)
        shot("tool_station_parts")
        print("go", json.dumps(ctl("click-at", ox + 172, oy + 52))[:120], flush=True)
        ctl("wait", 5)
        shot("tool_station_done")
        ctl("click-slot", 306, "shift")
        ctl("wait", 3)
        inv = ctl("inventory")
        print("after station", inv.get("counts"), flush=True)
        results.append(("tool station builds a pickaxe from parts", inv.get("counts", {}).get(f"{L}:pickaxe", 0) == 1))
        ctl("close")
        ctl("command", "clear @s")
        ctl("command", "tinkers tool pickaxe wood iron cobalt")
        ctl("command", "tinkers tool hammer wood ardite cobalt iron")
        ctl("command", "tinkers tool broadsword bone manyullyn knightslime")
        ctl("command", "tinkers tool shortbow wood wood string")
        ctl("command", "tinkers tool lumberaxe wood draconium ruby tin")
        ctl("wait", 5)
        move(tables["tool_station"][0] + 0.5, y - 1, z + 2.5)
        open_block(tables["tool_station"])
        shot("tool_station")
        print("menu", json.dumps(slots())[:500], flush=True)
        ctl("close")
        move(tables["tool_forge"][0] + 0.5, y - 1, z + 2.5)
        open_block(tables["tool_forge"])
        shot("tool_forge")
        ctl("close")
        ctl("open-inventory")
        time.sleep(0.5)
        shot("inventory")
        ctl("close")
        # The smeltery: walls, lava in a tank, iron melted, a faucet pouring.
        sx, sy, sz = x + 6, y - 1, z - 8
        for dy in range(0, 3):
            ctl("fill", sx, sy + dy, sz, sx + 4, sy + dy, sz + 4, f"{L}:seared_bricks")
            if dy:
                ctl("fill", sx + 1, sy + dy, sz + 1, sx + 3, sy + dy, sz + 3, "minecraft:air")
        controller = (sx + 2, sy + 1, sz + 4)
        ctl("setblock", *controller, f"{L}:smeltery_controller[facing=south]")
        ctl("fill", sx, sy + 1, sz + 1, sx, sy + 2, sz + 3, f"{L}:seared_glass")
        tank = (sx + 1, sy + 1, sz + 4)
        ctl("setblock", *tank, f"{L}:seared_tank")
        drain = (sx + 3, sy + 1, sz + 4)
        ctl("setblock", *drain, f"{L}:seared_drain[facing=south]")
        faucet = (sx + 3, sy + 1, sz + 5)
        ctl("setblock", *faucet, f"{L}:seared_faucet[facing=north]")
        table = (sx + 3, sy, sz + 5)
        ctl("setblock", *table, f"{L}:casting_table")
        move(sx + 1.5, sy, sz + 7.5)
        ctl("wait", 50)
        ctl("command", "clear @s")
        ctl("give", "minecraft:lava_bucket", 1)
        ctl("select", "minecraft:lava_bucket")
        ctl("look-at-block", *tank)
        ctl("wait", 3)
        ctl("mouse", "right", "click")
        ctl("wait", 5)
        for it, n in (("minecraft:iron_ingot", 12), ("minecraft:gold_ingot", 6)):
            ctl("command", f'summon minecraft:item {sx + 2} {sy + 2} {sz + 2} {{Item:{{id:"{it}",count:{n}}},PickupDelay:32000}}')
        time.sleep(14)
        ctl("command", "clear @s")
        move(controller[0] + 0.5, sy, sz + 7)
        open_block(controller)
        shot("smeltery_gui")
        print("menu", json.dumps(slots())[:300], flush=True)
        ctl("close")
        ctl("give", f"{L}:cast_pick_head", 1)
        ctl("select", f"{L}:cast_pick_head")
        move(table[0] + 0.5, sy, table[2] + 2.5)
        ctl("look-at-block", *table)
        ctl("wait", 3)
        ctl("mouse", "right", "click")
        ctl("wait", 5)
        ctl("look-at-block", *faucet)
        ctl("wait", 3)
        ctl("mouse", "right", "click")
        ctl("wait", 30)
        ctl("look-at", sx + 2.5, sy + 1, sz + 5)
        ctl("wait", 3)
        shot("smeltery_world")
        ctl("command", f"tp @s {sx + 2.5} {sy + 6} {sz + 2.5}")
        ctl("look-at", sx + 2.5, sy, sz + 2.5)
        ctl("wait", 20)
        shot("smeltery_top")
        print("log:", [l[:200] for l in open(f"{SCRATCH}/home/logs/latest.log", errors="replace") if "WARN" in l and "missing texture" in l][:12], flush=True)
        ctl("quit", timeout=30)
    finally:
        try:
            game.wait(timeout=30)
        except subprocess.TimeoutExpired:
            game.kill()
    print(sorted(os.listdir(f"{SCRATCH}/shots")))
    for name, ok in results:
        print(("PASS " if ok else "FAIL ") + name)
    return 0


if __name__ == "__main__":
    sys.exit(main())
