import unittest


class TestAgentDrl(unittest.TestCase):
    def _load_impl(self):
        import importlib.machinery
        import importlib.util
        import sys
        from pathlib import Path

        path = Path(__file__).resolve().parents[1] / "agent-drl" / "agent_drl.py"
        spec = importlib.util.spec_from_loader(
            "kairos_agent_drl_impl",
            importlib.machinery.SourceFileLoader("kairos_agent_drl_impl", str(path)),
        )
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        return module

    def test_normalize_observation(self):
        m = self._load_impl()
        _normalize_observation = m._normalize_observation

        self.assertEqual(_normalize_observation({"observation": [1, 2.0, "3"]}), [1.0, 2.0, 3.0])
        self.assertIsNone(_normalize_observation({"observation": "nope"}))

    def test_mock_policy_momentum(self):
        m = self._load_impl()
        _MockPolicy = m._MockPolicy

        p = _MockPolicy("momentum")
        a, c = p.predict([1.0])
        self.assertEqual(a, 1)
        a, c = p.predict([-1.0])
        self.assertEqual(a, 2)
        a, c = p.predict([0.0])
        self.assertEqual(a, 0)


if __name__ == "__main__":
    unittest.main()
