import unittest


class TestTrainDrlSb3(unittest.TestCase):
    def _load_impl(self):
        import importlib.machinery
        import importlib.util
        import sys
        from pathlib import Path

        path = Path(__file__).resolve().parents[1] / "train" / "train_drl_sb3.py"
        spec = importlib.util.spec_from_loader(
            "kairos_train_drl_sb3_impl",
            importlib.machinery.SourceFileLoader("kairos_train_drl_sb3_impl", str(path)),
        )
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        return module

    def test_policy_kwargs_for_net_arch(self):
        m = self._load_impl()
        f = m._policy_kwargs_for_net_arch
        self.assertEqual(f("small"), {"net_arch": [64, 64]})
        self.assertEqual(f("medium"), {"net_arch": [128, 128]})
        self.assertEqual(f("large"), {"net_arch": [256, 256]})

    def test_sharpe_like(self):
        m = self._load_impl()
        sharpe = m._sharpe_like
        self.assertEqual(sharpe([]), 0.0)
        self.assertEqual(sharpe([0.1]), 0.0)
        self.assertGreater(sharpe([0.02, 0.01, 0.03, 0.015]), 0.0)

    def test_model_path_for_trial_is_deterministic(self):
        m = self._load_impl()
        from pathlib import Path

        fn = m._model_path_for_trial
        a = fn(
            model_out_dir=Path("runs/models"),
            learning_rate=0.001,
            gamma=0.99,
            batch_size=128,
            entropy_coef=0.001,
            net_arch="medium",
            seed=42,
        )
        b = fn(
            model_out_dir=Path("runs/models"),
            learning_rate=0.001,
            gamma=0.99,
            batch_size=128,
            entropy_coef=0.001,
            net_arch="medium",
            seed=42,
        )
        self.assertEqual(str(a), str(b))
        self.assertTrue(str(a).endswith(".zip"))


if __name__ == "__main__":
    unittest.main()
