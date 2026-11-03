import argparse
import datetime
import json
import platform
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ANONYMOUS_LIMIT = 1.0
RSS_LIMIT = 2.0
ROOT = Path(__file__).resolve().parent.parent
DARWIN = sys.platform == "darwin"


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True)


def sysctl(name: str) -> str:
    return run(["sysctl", "-n", name]).stdout.strip()


def cpu_model() -> str:
    if DARWIN:
        return sysctl("machdep.cpu.brand_string")
    for line in Path("/proc/cpuinfo").read_text().splitlines():
        if line.startswith("model name"):
            return line.split(":", 1)[1].strip()
    return platform.processor()


def memory_bytes() -> int:
    if DARWIN:
        return int(sysctl("hw.memsize"))
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemTotal"):
            return int(line.split()[1]) * 1024
    return 0


def hardware() -> dict[str, str | int]:
    return {
        "os": f"{platform.system()} {platform.release()}",
        "arch": platform.machine(),
        "cpu": cpu_model(),
        "memory_bytes": memory_bytes(),
    }


def cli_binary() -> Path:
    run(["cargo", "build", "--release", "-p", "cadboss-cli"])
    metadata = json.loads(run(["cargo", "metadata", "--format-version", "1", "--no-deps"]).stdout)
    return Path(metadata["target_directory"]) / "release" / "cadboss"


def generate(entities: int, path: Path) -> None:
    run(
        [
            "cargo",
            "run",
            "--release",
            "-p",
            "cadboss-testkit",
            "--example",
            "gen_large",
            "--",
            "--entities",
            str(entities),
            "--out",
            str(path),
        ]
    )


def measure(binary: Path, path: Path) -> tuple[int, float]:
    flag = "-l" if DARWIN else "-v"
    done = subprocess.run(
        ["/usr/bin/time", flag, str(binary), "text", str(path)],
        cwd=ROOT,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    if DARWIN:
        peak = int(re.search(r"(\d+)\s+maximum resident set size", done.stderr).group(1))
        seconds = float(re.search(r"([\d.]+)\s+real", done.stderr).group(1))
        return peak, seconds
    peak = int(re.search(r"Maximum resident set size \(kbytes\): (\d+)", done.stderr).group(1)) * 1024
    clock = re.search(r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): ([\d:.]+)", done.stderr).group(1)
    seconds = sum(float(part) * 60**power for power, part in enumerate(reversed(clock.split(":"))))
    return peak, seconds


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--entities", type=int, required=True)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    binary = cli_binary()
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "large.dxf"
        generate(args.entities, path)
        input_bytes = path.stat().st_size
        peak, seconds = measure(binary, path)
    anonymous = max(peak - input_bytes, 0)
    anonymous_ratio = anonymous / input_bytes
    rss_ratio = peak / input_bytes
    result = {
        "benchmark": "text",
        "entities": args.entities,
        "input_bytes": input_bytes,
        "peak_rss_bytes": peak,
        "anonymous_bytes": anonymous,
        "anonymous_ratio": round(anonymous_ratio, 3),
        "rss_ratio": round(rss_ratio, 3),
        "seconds": seconds,
        "hardware": hardware(),
    }
    out = args.out or ROOT / "benchmarks" / f"results-{datetime.date.today().isoformat()}.json"
    out.write_text(json.dumps(result, indent=2) + "\n")
    print(
        f"text {args.entities} entities: input {input_bytes / 1e6:.1f} MB, "
        f"peak {peak / 1e6:.1f} MB, anonymous ratio {anonymous_ratio:.3f}, "
        f"rss ratio {rss_ratio:.3f}, {seconds:.2f} s -> {out}"
    )
    return int(anonymous_ratio > ANONYMOUS_LIMIT or rss_ratio > RSS_LIMIT)


if __name__ == "__main__":
    sys.exit(main())
