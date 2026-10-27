#!/usr/bin/env python3
"""Turn a raw recording into what the edit needs: the sync-marker time and a numbered JPEG frame sequence.

    python3 demo/record/post.py demo/out/blog-informed

Writes <dir>/footage.json (cue times in video seconds) and <dir>/frames/00001.jpg ... at the recording's fps,
scaled to the width the stage shows them at. Frame n shows video time (n - 1) / fps.
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

FRAME_W = 1600  # the pane is 1116 px wide on the 1920x1080 stage; extra pixels keep the zoomed shots sharp


def find_marker(video, meta):
    """First run of magenta frames in the top-left corner of the viewport: returns (start, end) in video seconds."""
    fps = meta["fps"]
    box = meta["viewport_in_crop"]
    side = int(48 * meta["dpr"])
    crop = f"crop={side - 8}:{side - 8}:4:{box['y'] + 4},scale=1:1:flags=area"
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(video), "-vf", crop, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
        capture_output=True, check=True,
    ).stdout
    hits = [i for i in range(len(raw) // 3) if raw[3 * i] > 190 and raw[3 * i + 1] < 110 and raw[3 * i + 2] > 190]
    if not hits:
        raise SystemExit("sync marker not found in the footage")
    start = hits[0]
    end = start
    while end + 1 in hits:
        end += 1
    return start / fps, (end + 1) / fps


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("directory")
    args = parser.parse_args()
    root = Path(args.directory)
    meta = json.loads((root / "meta.json").read_text())
    video = root / "raw.mp4"
    start, end = find_marker(video, meta)
    frames = root / "frames"
    frames.mkdir(exist_ok=True)
    for old in frames.glob("*.jpg"):
        old.unlink()
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-i", str(video), "-vf", f"fps={meta['fps']},scale={FRAME_W}:-2:flags=lanczos",
         "-q:v", "3", str(frames / "%05d.jpg")],
        check=True,
    )
    count = len(list(frames.glob("*.jpg")))
    footage = {
        "scenario": meta["scenario"], "fps": meta["fps"], "marker": [round(start, 3), round(end, 3)],
        "frames": count, "dir": str(frames.resolve()),
        # cue times measured from the marker, converted to video seconds
        "cues": {name: round(start + t, 3) for name, t in meta["cues"].items()},
    }
    (root / "footage.json").write_text(json.dumps(footage, indent=2))
    print(f"{meta['scenario']}: marker {start:.2f}-{end:.2f}s, {count} frames, cues {footage['cues']}")


if __name__ == "__main__":
    sys.exit(main())
