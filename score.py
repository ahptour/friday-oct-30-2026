#!/usr/bin/env python3
"""Speed Demon arithmetic. T is seconds, M bytes, S code lines, D = 1 + deps.

The calculator accepts measurements from any single, consistent environment.
Using official axes produces official-formula results; native axes are local.
"""
import argparse
import json
import math
from pathlib import Path


def validate(metrics, *, reference=False):
    for axis in "TMSD":
        value = metrics[axis]
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
            raise ValueError(f"{axis} must be a finite number")
        minimum = 1 if axis == "D" else 0
        if value < minimum or (value == 0 and (axis != "S" or reference)):
            raise ValueError(f"invalid {axis}: {value}")


def costs(metrics, reference):
    validate(reference, reference=True)
    validate(metrics)
    return {
        "m": max(metrics["M"] / reference["M"], 1 / 50),
        "s": max(metrics["S"] / reference["S"], 1),
        "d": max(metrics["D"] / reference["D"], 1),
    }


def score(metrics, reference):
    c = costs(metrics, reference)
    adjusted = metrics["T"] * c["m"] ** 0.4 * c["s"] ** 0.3 * c["d"] ** 0.3
    k = math.sqrt(reference["T"] / adjusted)
    raw = 50 * math.log10(k)
    return {
        "normalized_costs": c,
        "adjusted_runtime_s": adjusted,
        "K": k,
        "unclamped_machine_points": raw,
        "machine_points": min(100, max(0, raw)),
    }


def compare(old, new, reference, gain=5):
    if not math.isfinite(gain) or gain < 0:
        raise ValueError("target gain must be finite and nonnegative")
    a, b = costs(old, reference), costs(new, reference)
    contributions = {
        "T": 25 * math.log10(old["T"] / new["T"]),
        "M": 10 * math.log10(a["m"] / b["m"]),
        "S": 7.5 * math.log10(a["s"] / b["s"]),
        "D": 7.5 * math.log10(a["d"] / b["d"]),
    }
    budget = old["T"] * (a["m"] / b["m"]) ** 0.4 * (a["s"] / b["s"]) ** 0.3 * (a["d"] / b["d"]) ** 0.3
    delta = score(new, reference)["machine_points"] - score(old, reference)["machine_points"]
    return {
        "pre_clamp_axis_point_changes": contributions,
        "pre_clamp_point_change": sum(contributions.values()),
        "machine_point_change": delta,
        "final_track_point_change_if_judges_unchanged": 0.65 * delta,
        "break_even_runtime_s": budget,
        "target_gain_pre_clamp_points": gain,
        "target_runtime_s": budget * 10 ** (-gain / 25),
    }


def overall(points, judges=None):
    """Return a top-three scenario, explicitly machine-only without judges."""
    if any(not math.isfinite(p) or not 0 <= p <= 100 for p in points.values()):
        raise ValueError("machine points must be finite and between 0 and 100")
    if judges is not None:
        if any(not math.isfinite(judges[t]) or not 0 <= judges[t] <= 100 for t in points):
            raise ValueError("provide a judge score between 0 and 100 for every track")
        totals = {t: 0.65 * p + 0.35 * judges[t] for t, p in points.items()}
    else:
        totals = points
    best = sorted(totals, key=lambda t: (-totals[t], t))[:3]
    value = None
    if len(best) == 3:
        value = 0 if any(totals[t] == 0 for t in best) else math.exp(sum(math.log(totals[t]) for t in best) / 3)
    return {"basis": "final_with_supplied_judges" if judges is not None else "machine_only", "counting_tracks": best, "geometric_mean": value}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--reference", required=True, type=Path, help="JSON with T/M/S/D")
    ap.add_argument("--candidate", required=True, type=Path, help="JSON with T/M/S/D")
    ap.add_argument("--baseline", type=Path, help="previous T/M/S/D; defaults to reference")
    ap.add_argument("--gain", type=float, default=5, help="target pre-clamp machine-point gain")
    ap.add_argument("--track", choices=list("abcd"))
    ap.add_argument("--machine-points", type=Path, help="coherent track->machine points JSON for an aggregate scenario")
    ap.add_argument("--judges", type=Path, help="track->judge scores JSON; omitted means machine-only scenario")
    args = ap.parse_args()
    try:
        ref = json.loads(args.reference.read_text())
        candidate = json.loads(args.candidate.read_text())
        old = json.loads(args.baseline.read_text()) if args.baseline else ref
        result = {"candidate": score(candidate, ref), "comparison": compare(old, candidate, ref, args.gain)}
        if args.machine_points:
            if args.track is None:
                ap.error("--machine-points requires --track")
            points = json.loads(args.machine_points.read_text())
            judges = json.loads(args.judges.read_text()) if args.judges else None
            before = overall(points, judges)
            points[args.track] = result["candidate"]["machine_points"]
            after = overall(points, judges)
            delta = None if before["geometric_mean"] is None or after["geometric_mean"] is None else after["geometric_mean"] - before["geometric_mean"]
            result["aggregate_scenario"] = {"before": before, "after": after, "change": delta}
        elif args.judges:
            ap.error("--judges requires --machine-points")
        print(json.dumps(result, indent=2, allow_nan=False))
    except (OSError, ValueError, KeyError, TypeError) as exc:
        ap.exit(1, f"FAIL  {exc}\n")


if __name__ == "__main__":
    main()
