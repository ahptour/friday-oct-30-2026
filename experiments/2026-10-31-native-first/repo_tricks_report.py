#!/usr/bin/env python3
"""Summarize preserved native samples; keep contest projections hypothetical."""
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import statistics

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
HARNESS = ROOT / '.out/setup/harness/harness'
spec = importlib.util.spec_from_file_location('scoring', HARNESS / 'scoring.py')
scoring = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scoring)
feed_path = HERE / 'repo-tricks-board.json'
feed = json.loads(feed_path.read_text())
team = next(t for t in feed['teams'] if t['team'] == 'ahptour')
latest = {t: data['latest'] for t, data in team['tracks'].items()}
calibration = json.loads((HARNESS / 'calibration-seed1.json').read_text())['tracks']
static = json.loads((HARNESS / 'reference_axes.json').read_text())
refs = {t: dict(T=c['T_ref_s'], M=c['M_ref_bytes'], S=static[t]['S_lines'], D=1)
        for t, c in calibration.items()}


def axes(row):
    return dict(T=row['T_s'], M=row['M_bytes'], S=row['S'], D=row['D'])


def score(track, costs):
    ref = refs[track]
    m = max(costs['M'] / ref['M'], 1 / 50)
    s = max(costs['S'] / ref['S'], 1)
    d = max(costs['D'] / ref['D'], 1)
    adjusted_s = costs['T'] * m**0.4 * s**0.3 * d**0.3
    k = scoring.k_from_measurements(ref, costs)
    assert math.isclose(k, math.sqrt(ref['T'] / adjusted_s), rel_tol=1e-12)
    return dict(E_s=adjusted_s, K=k, points=scoring.speed_points(k))


def code_lines(name):
    # Local delta estimate, not the official formatter+scc pass across the track.
    return sum(bool(line.strip()) and not line.strip().startswith('//')
               for line in (HERE / f'{name}.rs').read_text().splitlines())


def aggregate(points, judges):
    totals = {t: scoring.track_total(p, judges[t]) for t, p in points.items()}
    return dict(G=scoring.overall(list(totals.values())), totals=totals,
                counting=sorted(totals, key=totals.get, reverse=True)[:3])


reports = [('b-repo-tricks-screen.json', 'b-clause-base'),
           ('b-inline-clauses-repeat.json', 'b-clause-base'),
           ('d-prune-screen.json', 'd-prune-base')]
rows = []
for filename, baseline in reports:
    report = json.loads((HERE / filename).read_text())
    track = baseline[0]
    old = axes(latest[track])
    for key, summary in report['summary'].items():
        name, seed = key.split('/seed')
        seed = int(seed)
        if name == baseline:
            continue
        base = report['summary'][f'{baseline}/seed{seed}']
        old_samples = {r['trial']: r['elapsed_ms'] for r in report['rows']
                       if r['variant'] == baseline and r['seed'] == seed and r['phase'] == 'timing'}
        new_samples = {r['trial']: r['elapsed_ms'] for r in report['rows']
                       if r['variant'] == name and r['seed'] == seed and r['phase'] == 'timing'}
        ratios = [new_samples[i] / old_samples[i] for i in old_samples]
        new_s = old['S'] + code_lines(name) - code_lines(baseline)
        time_ratio = summary['median_ms'] / base['median_ms']
        projected = {**old, 'T': old['T'] * time_ratio, 'S': new_s}
        before, after = score(track, old), score(track, projected)
        points = {t: r['points'] for t, r in latest.items()}
        hypothetical_points = {**points, track: after['points']}
        scenarios = {}
        for label, judges in [('equal_judges_80', dict.fromkeys(points, 80)),
                              ('B_judge_84_others_80', {**dict.fromkeys(points, 80), 'b': 84})]:
            a, b = aggregate(points, judges), aggregate(hypothetical_points, judges)
            scenarios[label] = dict(judges=judges, before=a, after=b, delta_G=b['G'] - a['G'])
        rows.append(dict(report=filename, variant=name, seed=seed,
                         measured_native=dict(T_median_ms=summary['median_ms'],
                                              baseline_T_median_ms=base['median_ms'],
                                              timing_samples=len(new_samples),
                                              faster_pairs=sum(r < 1 for r in ratios),
                                              paired_ratio_median=statistics.median(ratios),
                                              ratio_of_medians=time_ratio,
                                              sample_stdev_ms=statistics.stdev(new_samples.values()),
                                              range_ms=[min(new_samples.values()), max(new_samples.values())],
                                              max_RSS_bytes=summary['max_rss_bytes'],
                                              max_Mac_footprint_bytes=summary['max_peak_footprint_bytes'],
                                              baseline_max_RSS_bytes=base['max_rss_bytes'],
                                              baseline_max_Mac_footprint_bytes=base['max_peak_footprint_bytes']),
                         source=dict(baseline_official_S=old['S'], estimated_candidate_S=new_s,
                                     local_delta=code_lines(name)-code_lines(baseline)),
                         dependencies=1, official_candidate_M=None, native_E_K_points=None,
                         equal_official_memory_budget=dict(
                             max_runtime_ratio=(old['S']/new_s)**0.3,
                             break_even_Intel_ms=1000*old['T']*(old['S']/new_s)**0.3,
                             five_machine_point_target_Intel_ms=1000*old['T']*(old['S']/new_s)**0.3*10**(-5/25)),
                         hypothetical_only=dict(
                             assumptions='Native ratio transfers to Intel; official M and D unchanged; estimated S accepted. Not a measured candidate score.',
                             costs=projected, before=before, after=after,
                             clamped_point_delta=after['points']-before['points'],
                             preclamp_axis_points=dict(T=25*math.log10(1/time_ratio), M=0,
                                                      S=7.5*math.log10(old['S']/new_s), D=0),
                             aggregate_scenarios=scenarios)))

result = dict(board_generated_utc=feed['generated_at'],
              board_sha256=hashlib.sha256(feed_path.read_bytes()).hexdigest(),
              baseline_HEAD='1b8528789fb6cf7f6c847e8fabc566cc4923ad56',
              reference=refs,
              scoring_sha256=hashlib.sha256((HARNESS/'scoring.py').read_bytes()).hexdigest(),
              latest_official={t: dict(run_id=r['run_id'], commit=r['commit'], costs=axes(r),
                                       calculated=score(t, axes(r)), board_points=r['points'])
                               for t, r in latest.items()},
              native_limit='Mac time/RSS/footprint are not official T/M. Native E/K/points unavailable.',
              rows=rows)
(HERE / 'repo-tricks-analysis.json').write_text(json.dumps(result, indent=2) + '\n')
for row in rows:
    native, hypothetical = row['measured_native'], row['hypothetical_only']
    print(row['report'], row['variant'], row['seed'],
          f"{native['baseline_T_median_ms']:.3f} -> {native['T_median_ms']:.3f} ms",
          f"{native['faster_pairs']}/{native['timing_samples']} faster pairs",
          f"hypothetical equal-M point delta {hypothetical['clamped_point_delta']:+.3f}")
