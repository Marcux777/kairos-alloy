import threading
import time
import unittest


class TestOptimizeBayes(unittest.TestCase):
    def _load_impl(self):
        import importlib.machinery
        import importlib.util
        import sys
        from pathlib import Path

        path = Path(__file__).resolve().parents[1] / "train" / "optimize_bayes.py"
        spec = importlib.util.spec_from_loader(
            "kairos_optimize_bayes_impl",
            importlib.machinery.SourceFileLoader("kairos_optimize_bayes_impl", str(path)),
        )
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        return module

    def test_optimizer_finds_reasonable_region(self):
        m = self._load_impl()
        ParamSpec = m.ParamSpec
        optimize = m.optimize

        specs = [
            ParamSpec(name="x", kind="float", low=0.0, high=1.0),
            ParamSpec(name="y", kind="float", low=0.0, high=1.0),
        ]

        def objective(params):
            x = float(params["x"])
            y = float(params["y"])
            # Maximum is 0.0 at x=0.2,y=0.8
            score = -((x - 0.2) ** 2 + (y - 0.8) ** 2)
            return score, "synthetic"

        out = optimize(
            specs=specs,
            objective=objective,
            n_trials=24,
            init_random=6,
            parallelism=4,
            maximize=True,
            seed=42,
            candidate_pool=128,
            exploration=0.01,
        )
        self.assertEqual(out["successful_trials"], 24)
        self.assertGreater(out["best_score"], -0.25)
        self.assertIn("x", out["best_params"])
        self.assertIn("y", out["best_params"])

    def test_parallelism_uses_multiple_threads(self):
        m = self._load_impl()
        ParamSpec = m.ParamSpec
        optimize = m.optimize

        specs = [ParamSpec(name="x", kind="float", low=0.0, high=1.0)]
        thread_ids = set()
        lock = threading.Lock()

        def objective(params):
            _ = float(params["x"])
            with lock:
                thread_ids.add(threading.get_ident())
            time.sleep(0.01)
            return 1.0, "synthetic"

        out = optimize(
            specs=specs,
            objective=objective,
            n_trials=12,
            init_random=3,
            parallelism=4,
            maximize=True,
            seed=7,
            candidate_pool=64,
            exploration=0.0,
        )
        self.assertEqual(out["successful_trials"], 12)
        self.assertGreaterEqual(len(thread_ids), 2)

    def test_render_command_replaces_only_param_placeholders(self):
        m = self._load_impl()
        _render_command = m._render_command

        cmd = (
            "python3 -c \"import json; print(json.dumps({'score': 1.0}))\" "
            "--lr {learning_rate}"
        )
        argv = _render_command(cmd, {"learning_rate": 0.001})
        rendered = " ".join(argv)
        self.assertIn("{'score': 1.0}", rendered)
        self.assertIn("--lr", rendered)
        self.assertIn("0.001", rendered)


if __name__ == "__main__":
    unittest.main()
