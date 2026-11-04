import argparse
import datetime
import json
import platform
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DARWIN = sys.platform == "darwin"


@dataclass(frozen=True)
class Gate:
    anonymous: float
    rss: float


MAPPED_GATE = Gate(anonymous=1.0, rss=2.0)
OWNED_GATE = Gate(anonymous=2.0, rss=2.5)


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


def python_interpreter() -> Path:
    run(["uv", "run", "maturin", "develop", "--release"])
    return Path(run(["uv", "run", "python", "-c", "import sys; print(sys.executable)"]).stdout.strip())


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


def measure(command: list[str]) -> tuple[int, float]:
    flag = "-l" if DARWIN else "-v"
    done = subprocess.run(
        ["/usr/bin/time", flag, *command],
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


def record(
    benchmark: str,
    entities: int,
    input_bytes: int,
    measured: tuple[int, float],
    anonymous: int,
    gate: Gate,
) -> dict[str, object]:
    peak, seconds = measured
    return {
        "benchmark": benchmark,
        "entities": entities,
        "input_bytes": input_bytes,
        "peak_rss_bytes": peak,
        "anonymous_bytes": anonymous,
        "anonymous_ratio": round(anonymous / input_bytes, 3),
        "rss_ratio": round(peak / input_bytes, 3),
        "anonymous_gate": gate.anonymous,
        "rss_gate": gate.rss,
        "seconds": seconds,
    }


def passes(result: dict[str, object]) -> bool:
    return result["anonymous_ratio"] <= result["anonymous_gate"] and result["rss_ratio"] <= result["rss_gate"]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--entities", type=int, required=True)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    binary = cli_binary()
    python = python_interpreter()
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "large.dxf"
        generate(args.entities, path)
        input_bytes = path.stat().st_size
        cli = measure([str(binary), "text", str(path)])
        owned = measure([str(python), "-c", f"import cadboss; cadboss.Document.open({str(path)!r}).markdown()"])
    results = [
        record("cli-text", args.entities, input_bytes, cli, max(cli[0] - input_bytes, 0), MAPPED_GATE),
        record("py-open-markdown", args.entities, input_bytes, owned, owned[0], OWNED_GATE),
    ]
    out = args.out or ROOT / "benchmarks" / f"results-{datetime.date.today().isoformat()}.json"
    out.write_text(json.dumps({"runs": results, "hardware": hardware()}, indent=2) + "\n")
    for result in results:
        print(
            f"{result['benchmark']} {args.entities} entities: input {input_bytes / 1e6:.1f} MB, "
            f"peak {result['peak_rss_bytes'] / 1e6:.1f} MB, anonymous ratio {result['anonymous_ratio']:.3f} "
            f"(gate {result['anonymous_gate']}), rss ratio {result['rss_ratio']:.3f} (gate {result['rss_gate']}), "
            f"{result['seconds']:.2f} s"
        )
    print(f"-> {out}")
    return int(not all(passes(result) for result in results))


if __name__ == "__main__":
    sys.exit(main())
