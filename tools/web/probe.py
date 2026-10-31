"""Run the browser build in a real Chrome window and measure it, over the DevTools protocol.

python3 tools/web/probe.py [--url http://localhost:8080/] [--wait 25] [--measure 8] [--shot out.png]
                           [--phone] [--cpu 4]

Opens a fresh Chrome (its own profile, WebGPU on), loads the page, waits `wait` seconds for the
game to load and the match to start, then counts animation frames for `measure` seconds and
prints the frame rate (Bevy draws one frame per browser animation frame), the slowest frames,
and every console error. `--phone` emulates a Pixel 7a's screen (412 x 915 CSS px at 2.625x) and
touch; `--cpu N` slows the CPU N times (4 is roughly a mid-range phone against this desktop).
The GPU can't be slowed: phone GPU cost has to be judged from the desktop GPU's headroom.
"""

import argparse
import asyncio
import base64
import json
import os
import shutil
import subprocess
import tempfile
import time
import urllib.request

import websockets

MEASURE_JS = """
new Promise(resolve => {
  const times = [];
  let last = performance.now();
  const end = last + %d;
  function tick(now) {
    times.push(now - last); last = now;
    if (now < end) requestAnimationFrame(tick); else resolve(times);
  }
  requestAnimationFrame(tick);
})
"""


async def run(a):
    port = 9333
    profile = tempfile.mkdtemp(prefix="panga-chrome-")
    chrome = subprocess.Popen([
        shutil.which("google-chrome") or "google-chrome", f"--remote-debugging-port={port}", f"--user-data-dir={profile}",
        "--no-first-run", "--no-default-browser-check", "--enable-unsafe-webgpu", "--enable-features=Vulkan",
        "--ignore-gpu-blocklist", "--autoplay-policy=no-user-gesture-required", "--window-size=1300,800", "about:blank",
    ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(50):
            try:
                tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json"))
                page = next(t for t in tabs if t["type"] == "page")
                break
            except Exception:
                time.sleep(0.2)
        async with websockets.connect(page["webSocketDebuggerUrl"], max_size=None) as ws:
            seq = 0
            errors = []

            async def call(method, **params):
                nonlocal seq
                seq += 1
                me = seq
                await ws.send(json.dumps({"id": me, "method": method, "params": params}))
                while True:
                    msg = json.loads(await ws.recv())
                    if msg.get("method") == "Runtime.consoleAPICalled" and msg["params"]["type"] in ("error", "warning"):
                        errors.append(" ".join(str(x.get("value", x.get("description", ""))) for x in msg["params"]["args"])[:300])
                    if msg.get("method") == "Runtime.exceptionThrown":
                        errors.append(msg["params"]["exceptionDetails"].get("text", "") + " " +
                                      str(msg["params"]["exceptionDetails"].get("exception", {}).get("description", ""))[:300])
                    if msg.get("id") == me:
                        return msg.get("result", {})

            await call("Runtime.enable")
            await call("Page.enable")
            if a.phone:
                await call("Emulation.setDeviceMetricsOverride", width=412, height=915, deviceScaleFactor=2.625, mobile=True)
                await call("Emulation.setTouchEmulationEnabled", enabled=True, maxTouchPoints=5)
            if a.cpu > 1:
                await call("Emulation.setCPUThrottlingRate", rate=a.cpu)
            t0 = time.time()
            await call("Page.navigate", url=a.url)
            # Let it load; keep reading console messages meanwhile.
            while time.time() - t0 < a.wait:
                await call("Runtime.evaluate", expression="1")
                await asyncio.sleep(0.5)
            r = await call("Runtime.evaluate", expression=MEASURE_JS % int(a.measure * 1000), awaitPromise=True, returnByValue=True)
            times = r.get("result", {}).get("value") or []
            if len(times) > 2:
                times = times[1:]
                fps = len(times) / (sum(times) / 1000.0)
                srt = sorted(times)
                p95 = srt[int(len(srt) * 0.95)]
                print(f"[probe] {len(times)} frames: {fps:.1f} fps, median {srt[len(srt) // 2]:.1f} ms, 95th {p95:.1f} ms, worst {srt[-1]:.1f} ms")
            else:
                print("[probe] no frames measured")
            if a.shot:
                shot = await call("Page.captureScreenshot", format="png")
                with open(a.shot, "wb") as f:
                    f.write(base64.b64decode(shot["data"]))
                print(f"[probe] screenshot {a.shot}")
            for e in errors[:15]:
                print("[console]", e)
    finally:
        chrome.terminate()
        chrome.wait(timeout=10)
        shutil.rmtree(profile, ignore_errors=True)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--url", default="http://localhost:8080/")
    p.add_argument("--wait", type=float, default=25.0)
    p.add_argument("--measure", type=float, default=8.0)
    p.add_argument("--shot")
    p.add_argument("--phone", action="store_true")
    p.add_argument("--cpu", type=float, default=1.0)
    asyncio.run(run(p.parse_args()))


if __name__ == "__main__":
    main()
