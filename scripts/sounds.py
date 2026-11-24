#!/usr/bin/env python3
"""Generate the game's recorded sound effects with ByteDance Seed Audio on
OpenRouter, then trim and level them into assets/sounds/*.wav.

    scripts/sounds.py gen explode        # a new take of one sound: target/sounds/explode-N.mp3
    scripts/sounds.py gen explode -n 3   # three takes, to pick the best by ear
    scripts/sounds.py use explode 2      # trim and level take 2 into assets/sounds/explode.wav
    scripts/sounds.py use explode FILE   # or any audio file

`gen` needs OPENROUTER_API_KEY (about $0.15 per minute of audio; a take is
a few seconds). `use` needs only ffmpeg. The game embeds the .wav files, so
rebuild after `use`. Every sound has a code-made fallback in src/sound/synth.rs.
"""

import json
import os
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TAKES = ROOT / "target" / "sounds"
OUT = ROOT / "assets" / "sounds"
NO_VOICES = " Sound effect only, no voices, no music."

# name: (prompt, longest it may run in seconds, keep only the first sound
# when the model made several)
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
        with urllib.request.urlopen(request, timeout=120) as response:
            path = TAKES / f"{name}-{n}.mp3"
            path.write_bytes(response.read())
        print(path.relative_to(ROOT))


def silences(path: Path) -> list[tuple[float, float]]:
    log = ffmpeg_log(path, f"silencedetect=n={FLOOR_DB}dB:d=0.12")
    starts = [float(x) for x in re.findall(r"silence_start: ([\d.]+)", log)]
    ends = [float(x) for x in re.findall(r"silence_end: ([\d.]+)", log)]
    return list(zip(starts, ends + [float("inf")] * (len(starts) - len(ends))))


def ffmpeg_log(path: Path, filters: str, cut: list[str] | None = None) -> str:
    result = subprocess.run(
        ["ffmpeg", "-hide_banner", *(cut or []), "-i", str(path), "-ac", "1", "-af", filters, "-f", "null", "-"],
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stderr


def use(name: str, source: Path) -> None:
    _, longest, first_only = SOUNDS[name]
    gaps = silences(source)
    # Start just before the sound does.
    start = 0.0
    if gaps and gaps[0][0] < 0.01:
        start = max(gaps[0][1] - 0.005, 0.0)
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
    print(f"{out.relative_to(ROOT)}: {length:.2f}s from {start:.2f}s, peak raised {-1.0 - peak:+.1f} dB")


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
