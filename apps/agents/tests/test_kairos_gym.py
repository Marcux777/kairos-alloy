import unittest


class TestKairosGymTomlPatch(unittest.TestCase):
    def test_patch_config_sets_agent_url_and_run_id(self):
        from tools.kairos_gym import _patch_config_for_gym

        base = """[run]
run_id = "x"
symbol = "BTC-USDT"
timeframe = "1h"
initial_capital = 10000.0

[agent]
mode = "hold"
url = "http://127.0.0.1:8000"
timeout_ms = 1000
retries = 0
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"
"""
        out = _patch_config_for_gym(
            base,
            run_id="gym_run",
            agent_url="http://127.0.0.1:9999",
            out_dir=None,
            force_report_html_off=True,
        )
        self.assertIn('run_id = "gym_run"', out)
        self.assertIn('mode = "remote"', out)
        self.assertIn('url = "http://127.0.0.1:9999"', out)
        self.assertIn("[report]", out)
        self.assertIn("html = false", out)

    def test_make_single_split_sweep_toml(self):
        from tools.kairos_gym import _make_single_split_sweep_toml
        from pathlib import Path

        raw = _make_single_split_sweep_toml(
            sweep_id="s",
            mode="backtest",
            base_config_path=Path("/tmp/cfg.toml"),
            split_id="episode",
            split_start="2024-01-01T00:00:00Z",
            split_end="2024-02-01T00:00:00Z",
        )
        self.assertIn('[base]', raw)
        self.assertIn('config = "/tmp/cfg.toml"', raw)
        self.assertIn('mode = "backtest"', raw)
        self.assertIn('[[splits]]', raw)
        self.assertIn('start = "2024-01-01T00:00:00Z"', raw)
        self.assertIn('end = "2024-02-01T00:00:00Z"', raw)


if __name__ == "__main__":
    unittest.main()

