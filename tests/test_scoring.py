import math
import unittest

from score import compare, overall, score


class ScoringTests(unittest.TestCase):
    def setUp(self):
        self.ref = {"T": 32.047276, "M": 227983360, "S": 56, "D": 1}

    def test_reference_and_points_anchors(self):
        self.assertAlmostEqual(score(self.ref, self.ref)["K"], 1)
        self.assertEqual(score(self.ref, self.ref)["machine_points"], 0)
        for factor, expected in ((100, 50), (10000, 100), (1e6, 100), (0.1, 0)):
            candidate = dict(self.ref, T=self.ref["T"] / factor)
            self.assertAlmostEqual(score(candidate, self.ref)["machine_points"], expected)

    def test_exact_capped_product_including_zero_source(self):
        for memory in (self.ref["M"] * 2, self.ref["M"] / 2, self.ref["M"] / 50, 1):
            for source in (0, 28, 56, 465):
                candidate = {"T": 0.045323, "M": memory, "S": source, "D": 2}
                expected = (self.ref["T"] / candidate["T"]) ** 0.5 * min(self.ref["M"] / memory, 50) ** 0.2 * min(self.ref["S"] / source if source else 1, 1) ** 0.15 * 0.5 ** 0.15
                self.assertAlmostEqual(score(candidate, self.ref)["K"], expected)

    def test_compact_record_loss_and_budget(self):
        old = {"T": 0.049090, "M": 19177472, "S": 272, "D": 1}
        new = {"T": 0.045323, "M": 25317376, "S": 465, "D": 1}
        result = compare(old, new, self.ref)
        self.assertAlmostEqual(result["machine_point_change"], -2.086045, places=5)
        self.assertAlmostEqual(result["break_even_runtime_s"], 0.037400, places=6)
        for axis, expected in {"T": 0.866860, "M": -1.206273, "S": -1.746630, "D": 0}.items():
            self.assertAlmostEqual(result["pre_clamp_axis_point_changes"][axis], expected, places=5)

    def test_floors_budget_and_clamped_delta(self):
        old = dict(self.ref, M=1, S=0, T=self.ref["T"] / 1e8)
        new = dict(old, M=0.5, T=old["T"] / 2)
        result = compare(old, new, self.ref)
        self.assertEqual(result["pre_clamp_axis_point_changes"]["M"], 0)
        self.assertEqual(result["machine_point_change"], 0)
        self.assertAlmostEqual(result["pre_clamp_point_change"], 25 * math.log10(2))
        result = compare(self.ref, dict(self.ref, M=self.ref["M"] * 2), self.ref)
        self.assertAlmostEqual(result["break_even_runtime_s"] / self.ref["T"], 2 ** -0.4)
        self.assertAlmostEqual(result["target_runtime_s"] / result["break_even_runtime_s"], 10 ** -0.2)

    def test_top_three_reselects_and_judges_can_change_set(self):
        points = {"a": 77.892, "b": 61.527, "c": 69.780, "d": 63.181}
        self.assertEqual(overall(points)["counting_tracks"], ["a", "c", "d"])
        points["b"] = 64
        self.assertEqual(overall(points)["counting_tracks"], ["a", "c", "b"])
        judged = overall(points, {"a": 0, "b": 100, "c": 0, "d": 0})
        self.assertEqual(judged["counting_tracks"][0], "b")
        self.assertIsNone(overall({"a": 10})["geometric_mean"])
        self.assertEqual(overall({"a": 10, "b": 10, "c": 0})["geometric_mean"], 0)

    def test_invalid_axes_do_not_produce_scores(self):
        for axis, value in (("T", 0), ("M", -1), ("S", -1), ("D", 0), ("T", math.nan), ("M", math.inf), ("D", True)):
            with self.subTest(axis=axis, value=value), self.assertRaises(ValueError):
                score(dict(self.ref, **{axis: value}), self.ref)


if __name__ == "__main__":
    unittest.main()
