# SPDX-License-Identifier: GPL-3.0-only
"""Synthetic logs for the pure serial menu-query trace validator."""
import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    'menu_engine_trace', ROOT / 'menu_engine_trace.py')
menu_engine_trace = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(menu_engine_trace)


def valid_trace():
    return '\n'.join([
        'INPUT_RECEIVED monotonic_ns=1000 cpu_ns=2000',
        'MENU_RPC_SENT id=41',
        'MENU_RPC_RETURN id=41 ok=true catalog_rev=7 count=320',
        'MENU_RESULT ok=true catalog_rev=7 count=320',
        'UI_COMMIT generation=3 update=4 source_error=false',
        'UI_PRESENTED generation=3 monotonic_ns=1100 refresh_ns=8333333 companion=none update=4 cpu_ns=2100',
    ])


class SerialKeyTraceTests(unittest.TestCase):
    def parse(self, text=None, expected_catalog_rev=7, profile_mode='clocks'):
        return menu_engine_trace.parse_serial_key_trace(
            valid_trace() if text is None else text, expected_catalog_rev, profile_mode)

    def test_serial_key_trace_accepts_exact_match(self):
        row = self.parse()
        self.assertEqual(row['rpc_id'], 41)
        self.assertEqual(row['catalog_rev'], 7)
        self.assertEqual(row['result_count'], 320)
        self.assertEqual((row['update'], row['input_ns'], row['presented_ns']), (4, 1000, 1100))
        self.assertAlmostEqual(row['latency_ms'], 0.0001)
        self.assertAlmostEqual(row['ui_cpu_ms'], 0.0001)
        self.assertEqual(row['refresh_ns'], 8333333)

    def test_serial_key_trace_ignores_unrelated_presentations(self):
        text = '\n'.join([
            'UI_PRESENTED generation=99 monotonic_ns=1050 refresh_ns=0 companion=none update=99 cpu_ns=2050',
            valid_trace(),
            'UI_PRESENTED generation=100 monotonic_ns=1200 refresh_ns=0 companion=none update=100 cpu_ns=2200',
        ])
        self.assertEqual(self.parse(text)['update'], 4)

    def test_serial_key_trace_rejects_wrong_generation_or_update(self):
        for old, new in (
            ('UI_PRESENTED generation=3 monotonic_ns=', 'UI_PRESENTED generation=8 monotonic_ns='),
            ('update=4 cpu_ns=2100', 'update=8 cpu_ns=2100'),
        ):
            with self.subTest(new=new), self.assertRaisesRegex(AssertionError, 'exact committed/presented update'):
                self.parse(valid_trace().replace(old, new))
        before_commit = valid_trace().replace(
            'UI_COMMIT generation=3 update=4 source_error=false\n'
            'UI_PRESENTED generation=3 monotonic_ns=1100 refresh_ns=8333333 companion=none update=4 cpu_ns=2100',
            '',
        )
        before_commit = before_commit.replace(
            'MENU_RESULT ok=true catalog_rev=7 count=320\n',
            'MENU_RESULT ok=true catalog_rev=7 count=320\n'
            'UI_PRESENTED generation=3 monotonic_ns=1050 refresh_ns=8333333 companion=none update=4 cpu_ns=2050\n'
            'UI_COMMIT generation=3 update=4 source_error=false\n',
        )
        with self.assertRaisesRegex(AssertionError, 'exact committed/presented update'):
            self.parse(before_commit)

    def test_serial_key_trace_checks_bounded_busy_chain(self):
        for total, valid in ((1, True), (11, True), (12, False)):
            with self.subTest(total=total):
                lines = ['INPUT_RECEIVED monotonic_ns=1000 cpu_ns=2000']
                lines.extend(f'MENU_RPC_SENT id={index}' for index in range(1, total + 1))
                lines.extend(
                    f'MENU_RPC_RETURN id={index} ok=false code=busy retry=true'
                    for index in range(1, total)
                )
                lines.append(f'MENU_RPC_RETURN id={total} ok=true catalog_rev=7 count=320')
                lines.extend([
                    'MENU_RESULT ok=true catalog_rev=7 count=320',
                    'UI_COMMIT generation=3 update=4 source_error=false',
                    'UI_PRESENTED generation=3 monotonic_ns=1100 refresh_ns=8333333 companion=none update=4 cpu_ns=2100',
                ])
                if valid:
                    self.assertEqual(self.parse('\n'.join(lines))['busy_retries'], total - 1)
                else:
                    with self.assertRaisesRegex(AssertionError, 'missing/extraneous completed menu RPC'):
                        self.parse('\n'.join(lines))
        duplicate = valid_trace().replace(
            'MENU_RPC_SENT id=41', 'MENU_RPC_SENT id=41\nMENU_RPC_SENT id=41')
        with self.assertRaisesRegex(AssertionError, 'bounded retryable busy replies'):
            self.parse(duplicate)

    def test_serial_key_trace_rejects_missing_or_duplicate_success(self):
        line = 'MENU_RPC_RETURN id=41 ok=true catalog_rev=7 count=320'
        missing = valid_trace().replace(line + '\n', '')
        duplicate = valid_trace().replace(line, line + '\n' + line)
        for text in (missing, duplicate):
            with self.subTest(text=text), self.assertRaisesRegex(AssertionError, 'missing/extraneous completed menu RPC'):
                self.parse(text)

    def test_serial_key_trace_rejects_nonmonotonic_clocks(self):
        for old, new in (
            ('monotonic_ns=1000 cpu_ns=2000', 'monotonic_ns=1100 cpu_ns=2000'),
            ('monotonic_ns=1100 refresh_ns=8333333', 'monotonic_ns=1000 refresh_ns=8333333'),
            ('update=4 cpu_ns=2100', 'update=4 cpu_ns=2000'),
        ):
            with self.subTest(new=new), self.assertRaisesRegex(AssertionError, 'invalid input/presentation clocks'):
                self.parse(valid_trace().replace(old, new))

    def test_serial_key_trace_checks_profile_mode(self):
        self.assertEqual(self.parse(profile_mode='1')['profiles'], [])
        profiled = valid_trace() + '\nINPUT_PROFILE stage=layout wall_ms=0.4 cpu_ms=0.2'
        self.assertEqual(self.parse(profiled, profile_mode='1')['profiles'], [
            {'stage': 'layout', 'wall_ms': 0.4, 'cpu_ms': 0.2},
        ])
        with self.assertRaisesRegex(AssertionError, 'clock-only capture emitted stage traces'):
            self.parse(profiled)
        with self.assertRaisesRegex(AssertionError, 'unknown typing profile mode'):
            self.parse(profile_mode='other')


if __name__ == '__main__':
    unittest.main()
