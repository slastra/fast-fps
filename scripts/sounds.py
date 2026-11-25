#!/usr/bin/env python3
"""Generate the game's recorded sound effects with ByteDance Seed Audio on
OpenRouter, then trim and level them into assets/sounds/*.wav.

    scripts/sounds.py gen explode        # a new take of one sound: target/sounds/explode-N.mp3
    scripts/sounds.py gen explode -n 3   # three takes, to pick the best by ear
    scripts/sounds.py use explode 2      # trim and level take 2 into assets/sounds/explode.wav
    scripts/sounds.py use explode FILE   # or any audio file

Some sounds are mixed from two: `use shotgun` lays the current pump.wav over
the shot, so pick the pump first.

`gen` needs OPENROUTER_API_KEY (about $0.15 per minute of audio; a take is
a few seconds). `use` needs only ffmpeg. The game embeds the .wav files, so
rebuild after `use`.
"""

import json
import os
import re
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TAKES = ROOT / "target" / "sounds"
OUT = ROOT / "assets" / "sounds"
NO_VOICES = " Sound effect only, no voices, no music."

# name: (prompt, longest it may run in seconds, keep only the first sound
# when the model made several[, (another sound to mix in, seconds after the
# start)]). The model puts a gun's pump or bolt seconds after the shot, so
# those are made on their own and mixed in where the game wants them.
SOUNDS = {
    "explode": (
        "A single hand grenade explosion outdoors: a sharp, deep boom with a punchy "
        "low-end thump, debris pattering down, short echo off nearby stone walls. "
        "About two seconds.",
        2.5,
        False,
    ),
    "knife": (
        "A single fast knife slash whooshing through the air, short and crisp, under "
        "half a second.",
        0.6,
        True,
    ),
    "throw": (
        "An arm throwing a hand grenade: one quick, soft cloth-and-air whoosh. Very short.",
        0.7,
        True,
    ),
    "bounce": (
        "A metal hand grenade bouncing twice on hard ground: two dull metallic clinks "
        "and a short roll.",
        # The game plays this on every landing, so keep the first clink.
        0.3,
        True,
    ),
    "pin": (
        "Pulling the pin from a hand grenade: a small metallic ring click and the "
        "spring lever release, very short.",
        0.8,
        False,
    ),
    "flesh": (
        "A knife stab hitting a body: a single short, dull, wet thud with a little "
        "cloth rustle. Video game hit sound, very short. No screams.",
        0.5,
        True,
    ),
    "wall": (
        "A steel knife blade striking a stone wall: one sharp metallic clang with a "
        "brief scrape. Very short.",
        0.8,
        True,
    ),
    "ak": (
        "A single AK-47 rifle shot outdoors: a sharp, heavy crack with a deep punchy thump "
        "and a short echo tail. One shot only.",
        0.6,
        True,
    ),
    "mp5": (
        "A single MP5 submachine gun shot: a lighter, snappy 9mm pop with a quick "
        "mechanical clack and a short tail. One shot only.",
        0.4,
        True,
    ),
    "glock": (
        "A single Glock 9mm pistol shot: a snappy, crisp pop with a short tail. One shot only.",
        0.4,
        True,
    ),
    "shotgun": (
        "A single pump-action shotgun blast: a big deep boom with a rolling tail. One shot only.",
        1.1,
        False,
        ("pump", 0.45),
    ),
    "pump": (
        "A pump-action shotgun being racked: two quick, solid mechanical clacks, back and "
        "forward. Very short.",
        0.5,
        False,
    ),
    "sniper": (
        "A single bolt-action sniper rifle shot: a huge sharp crack with a long rolling "
        "echo. One shot only.",
        1.4,
        False,
        ("bolt", 0.65),
    ),
    "bolt": (
        "A bolt-action rifle's bolt being worked: up, back, forward, down, three or four "
        "crisp metallic clicks. Under one second.",
        0.8,
        False,
    ),
    "hit": (
        "A video game hit marker: one tiny, crisp, high-pitched tick, like a small metal "
        "tap. Extremely short.",
        0.15,
        True,
    ),
    "kill": (
        "A video game kill confirmation: a short, bright, satisfying two-note chime, rising.",
        0.5,
        False,
    ),
    "click": (
        "A single small mechanical click, like a gun's trigger on an empty chamber. "
        "Extremely short.",
        0.12,
        True,
    ),
    "reload": (
        "A rifle magazine reload: the magazine released and pulled out, a new magazine "
        "inserted and seated with a click. Under one second.",
        1.3,
        False,
    ),
    "zoom": (
        "A sniper scope zooming in: one quick, soft mechanical click. Very short.",
        0.3,
        True,
    ),
    "flintlock": (
        "A single antique flintlock pistol shot: a quick fizzing whoosh of the powder pan, "
        "then a deep, booming black-powder blast with a soft rolling echo. One shot only.",
        1.6,
        False,
    ),
    "flint_pan": (
        "A flintlock hammer striking the frizzen: a sharp flinty snap and a short fizzing "
        "sizzle of priming powder. Very short.",
        0.4,
        True,
    ),
    "flint_reload": (
        "Reloading an antique flintlock pistol: powder poured from a flask, a ramrod sliding "
        "down the barrel and tamping twice, then the hammer cocked back with a click. About "
        "two and a half seconds.",
        2.8,
        False,
    ),
    "duel_mark": (
        "A single low, deep war drum beat, dry and solemn. One hit only.",
        0.9,
        True,
    ),
    "duel_draw": (
        "A single strike of an old town church bell, one clear loud tone ringing out and "
        "fading. One strike only.",
        2.5,
        False,
    ),
}

RATE = 44100
# Quieter than this counts as silence when trimming.
FLOOR_DB = -45
FADE = 0.05


def gen(name: str, count: int) -> None:
    key = os.environ.get("OPENROUTER_API_KEY")
    if not key:
        sys.exit("OPENROUTER_API_KEY is not set")
    prompt = SOUNDS[name][0] + NO_VOICES
    TAKES.mkdir(parents=True, exist_ok=True)
    for _ in range(count):
        n = 1
        while (TAKES / f"{name}-{n}.mp3").exists():
            n += 1
        body = json.dumps(
            {
                "model": "bytedance-seed/seed-audio-1-0",
                "input": prompt,
                "response_format": "mp3",
            }
        ).encode()
        request = urllib.request.Request(
            "https://openrouter.ai/api/v1/audio/speech",
            data=body,
            headers={"Content-Type": "application/json", "Authorization": f"Bearer {key}"},
        )
        path = TAKES / f"{name}-{n}.mp3"
        path.write_bytes(fetch(request))
        print(path.relative_to(ROOT))


def fetch(request: urllib.request.Request) -> bytes:
    # Busy (429) and upstream failures (502, not billed) are worth retrying.
    for attempt in range(6):
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            if error.code not in (429, 502, 503) or attempt == 5:
                sys.exit(f"{error.code}: {error.read().decode(errors='replace')}")
            time.sleep(2 ** attempt * 2)
    raise AssertionError("unreachable")


def silences(path: Path) -> list[tuple[float, float]]:
    log = ffmpeg_log(path, f"silencedetect=n={FLOOR_DB}dB:d=0.12")
    starts = [float(x) for x in re.findall(r"silence_start: ([\d.]+)", log)]
    ends = [float(x) for x in re.findall(r"silence_end: ([\d.]+)", log)]
    return list(zip(starts, ends + [float("inf")] * (len(starts) - len(ends))))


def onset(path: Path) -> float:
    pcm = subprocess.run(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", str(path), "-ac", "1", "-ar", str(RATE),
         "-f", "s16le", "-"],
        capture_output=True,
        check=True,
    ).stdout
    samples = [abs(x) for x in struct.unpack(f"<{len(pcm) // 2}h", pcm)]
    threshold = max(samples) * 10 ** (-30 / 20)
    first = next(i for i, x in enumerate(samples) if x >= threshold)
    return first / RATE


def ffmpeg_log(path: Path, filters: str, cut: list[str] | None = None) -> str:
    result = subprocess.run(
        ["ffmpeg", "-hide_banner", *(cut or []), "-i", str(path), "-ac", "1", "-af", filters, "-f", "null", "-"],
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stderr


def use(name: str, source: Path) -> None:
    _, longest, first_only, *then = SOUNDS[name]
    gaps = silences(source)
    # Start just before the sound does. A gunshot that starts late feels like
    # lag, so this goes by the first sample within 30 dB of the peak rather
    # than by silence, which a little hiss before the sound would hide.
    start = max(onset(source) - 0.005, 0.0)
    # End where it falls silent for good, or after its first sound.
    end = start + longest
    for gap_start, gap_end in gaps:
        if gap_start <= start:
            continue
        if first_only or gap_end == float("inf"):
            end = min(end, gap_start + FADE)
            break
    length = end - start
    cut = ["-ss", f"{start:.3f}", "-t", f"{length:.3f}"]
    peak = float(re.search(r"max_volume: ([-\d.]+) dB", ffmpeg_log(source, "volumedetect", cut)).group(1))
    OUT.mkdir(parents=True, exist_ok=True)
    out = OUT / f"{name}.wav"
    subprocess.run(
        [
            # Seek on the input, so the fade's times count from the cut.
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
            *cut, "-i", str(source),
            "-ac", "1", "-ar", str(RATE), "-c:a", "pcm_s16le",
            "-af", f"volume={-1.0 - peak:.2f}dB,afade=t=out:st={max(length - FADE, 0):.3f}:d={FADE}",
            str(out),
        ],
        check=True,
    )
    note = ""
    if then:
        part, at = then[0]
        mix(out, OUT / f"{part}.wav", at)
        note = f", with {part}.wav at {at}s"
    print(f"{out.relative_to(ROOT)}: {length:.2f}s from {start:.2f}s, peak raised {-1.0 - peak:+.1f} dB{note}")


def mix(out: Path, part: Path, at: float) -> None:
    """Lay `part` over `out`, starting `at` seconds in, and level the result."""
    if not part.exists():
        sys.exit(f"{part.relative_to(ROOT)} is missing: run `use {part.stem}` first")
    both = f"[1]adelay={int(at * 1000)}[p];[0][p]amix=inputs=2:normalize=0:duration=longest"
    peak = float(re.search(r"max_volume: ([-\d.]+) dB", subprocess.run(
        ["ffmpeg", "-hide_banner", "-i", str(out), "-i", str(part), "-filter_complex",
         both + ",volumedetect", "-f", "null", "-"],
        capture_output=True, text=True, check=True).stderr).group(1))
    mixed = out.with_suffix(".mix.wav")
    subprocess.run(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(out), "-i", str(part),
         "-filter_complex", f"{both},volume={-1.0 - peak:.2f}dB",
         "-ac", "1", "-ar", str(RATE), "-c:a", "pcm_s16le", str(mixed)],
        check=True,
    )
    mixed.replace(out)


def main() -> None:
    args = sys.argv[1:]
    if len(args) < 2 or args[0] not in ("gen", "use") or args[1] not in SOUNDS:
        sys.exit(__doc__.strip() + "\n\nsounds: " + ", ".join(SOUNDS))
    name = args[1]
    if args[0] == "gen":
        count = int(args[3]) if len(args) > 3 and args[2] == "-n" else 1
        gen(name, count)
    else:
        take = args[2] if len(args) > 2 else "1"
        source = TAKES / f"{name}-{take}.mp3" if take.isdigit() else Path(take)
        use(name, source)


if __name__ == "__main__":
    main()
