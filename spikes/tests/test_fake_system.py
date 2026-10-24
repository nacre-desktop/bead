#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0083: the parity harness stand-ins for pactl, UPower and power-profiles-daemon answer what the daemon reads."""
import importlib.util
import io
import json
import pathlib
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[1]


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


fake = load_module("fake_system", ROOT / "fake_system.py")


def run(*argv, directory="/nonexistent"):
    out = io.StringIO()
    status = fake.pactl(list(argv), directory, out)
    return status, out.getvalue()


class PactlStandIn(unittest.TestCase):
    def listing(self, kind, directory="/nonexistent"):
        status, text = run("--format=json", "list", kind, directory=directory)
        self.assertEqual(status, 0)
        return json.loads(text)

    def test_two_sinks_and_one_source_with_the_daemons_identity_and_volume_fields(self):
        sinks, sources = self.listing("sinks"), self.listing("sources")
        self.assertEqual([s["description"] for s in sinks], ["Fixture speakers", "Fixture headphones"])
        self.assertEqual([s["description"] for s in sources], ["Fixture microphone"])
        for entry in sinks + sources:
            # N0059 reads node.name, name, description, mute (bool) and the largest channel value / 65536.
            self.assertTrue(entry["properties"]["node.name"])
            self.assertIsInstance(entry["mute"], bool)
            self.assertNotEqual(entry["properties"]["device.class"], "monitor")
            self.assertNotIn("monitor_of_sink", entry)
            level = max(c["value"] for c in entry["volume"].values()) / 65536
            self.assertAlmostEqual(level, 0.45, places=3)
        self.assertEqual(len({e["properties"]["node.name"] for e in sinks + sources}), 3)

    def test_defaults_resolve_to_listed_pulse_names(self):
        sinks, sources = self.listing("sinks"), self.listing("sources")
        self.assertEqual(run("get-default-sink"), (0, "fixture-speakers\n"))
        self.assertIn("fixture-speakers", [s["name"] for s in sinks])
        self.assertEqual(run("get-default-source"), (0, "fixture-microphone\n"))
        self.assertIn("fixture-microphone", [s["name"] for s in sources])

    def test_levels_follow_the_files_the_harness_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            pathlib.Path(directory, "sink").write_text("0.62\n")
            self.assertAlmostEqual(self.listing("sinks", directory)[0]["volume"]["front-left"]["value"] / 65536, 0.62, places=3)
            self.assertAlmostEqual(self.listing("sources", directory)[0]["volume"]["mono"]["value"] / 65536, 0.45, places=3)
            pathlib.Path(directory, "source").write_text("garbage\n")
            self.assertAlmostEqual(self.listing("sources", directory)[0]["volume"]["mono"]["value"] / 65536, 0.45, places=3)

    def test_subscribe_follows_the_events_file_and_other_commands_are_silent(self):
        with mock.patch("os.execvp") as execvp:
            fake.pactl(["subscribe"], "/d")
        execvp.assert_called_once_with("tail", ["tail", "-n", "0", "-F", "/d/events"])
        self.assertEqual(run("set-sink-volume", "@DEFAULT_SINK@", "50%"), (0, ""))
        self.assertEqual(run("--format=json", "list", "cards"), (0, ""))


class PipeWireDropIn(unittest.TestCase):
    def test_the_qml_arms_pipewire_has_the_same_three_nodes(self):
        conf = fake.pipewire_conf()
        self.assertEqual(conf.count("support.null-audio-sink"), 3)
        for name in ("fixture-speakers", "fixture-headphones", "fixture-microphone"):
            self.assertIn(f'node.name = "{name}"', conf)
        self.assertEqual(conf.count("media.class = Audio/Sink"), 2)
        self.assertEqual(conf.count("media.class = Audio/Source"), 1)
        self.assertTrue(conf.startswith("context.objects = [") and conf.endswith("]\n"))
        self.assertIn("factory = metadata", conf)  # the "default" metadata Quickshell reads defaults from
        self.assertIn("metadata.name = default", conf)

    def test_default_sink_and_source_are_listed_nodes(self):
        defaults = dict(fake.default_metadata())
        self.assertEqual(json.loads(defaults["default.audio.sink"]), {"name": "fixture-speakers"})
        self.assertEqual(json.loads(defaults["default.audio.source"]), {"name": "fixture-microphone"})


class FixedMachine(unittest.TestCase):
    def test_battery_is_63_percent_discharging_with_2h10m_left(self):
        battery = fake.BATTERY
        self.assertEqual((battery["percentage"], battery["state"], battery["on_battery"]), (63.0, 2, True))
        self.assertEqual(battery["time_to_empty"], 130 * 60)
        self.assertEqual(battery["time_to_full"], 0)  # unknown, as UPower reports while discharging

    def test_profiles_are_balanced_with_saver_and_performance(self):
        self.assertEqual(fake.PROFILES["active"], "balanced")
        self.assertEqual(fake.PROFILES["profiles"], ["power-saver", "balanced", "performance"])

    def test_the_bus_service_serves_those_values_on_both_profile_names(self):
        source = (ROOT / "fake-power.py").read_text()
        for needle in ["org.freedesktop.UPower.PowerProfiles", "net.hadess.PowerProfiles", "DisplayDevice",
                       "fake_system.BATTERY", "fake_system.PROFILES", "private bus only"]:
            self.assertIn(needle, source)
        compile(source, "fake-power.py", "exec")


if __name__ == "__main__":
    unittest.main()
