import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'detach-ghost-proof.py'
spec = importlib.util.spec_from_file_location('detach_ghost_proof', SCRIPT)
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)

CLOSED = '''\
[1.0]  -> wl_compositor#4.create_surface(new id wl_surface#10)
[1.1]  -> zwlr_layer_shell_v1#7.get_layer_surface(new id zwlr_layer_surface_v1#11, wl_surface#10, wl_output#9, 2, "bead-dev")
[1.2]  -> wl_surface#10.attach(wl_buffer#12, 0, 0)
[1.3]  -> wl_surface#10.commit()
'''
OPEN_PANEL = '''\
[2.0]  -> wl_compositor#4.create_surface(new id wl_surface#20)
[2.1]  -> zwlr_layer_shell_v1#7.get_layer_surface(new id zwlr_layer_surface_v1#21, wl_surface#20, wl_output#9, 2, "bead-dev")
[2.2]  -> wl_surface#20.attach(wl_buffer#22, 0, 0)
[2.3]  -> wl_surface#20.commit()
'''
PREPARED = '''\
[3.0]  -> wl_compositor#4.create_surface(new id wl_surface#30)
[3.1]  -> xdg_wm_base#8.get_xdg_surface(new id xdg_surface#31, wl_surface#30)
[3.2]  -> wl_surface#30.attach(wl_buffer#32, 0, 0)
[3.3]  -> wl_surface#30.commit()
'''
UNMAP_PANEL = '''\
[4.0]  -> wl_surface#20.attach(nil, 0, 0)
[4.1]  -> wl_surface#20.commit()
'''


class MappedSurfaceTests(unittest.TestCase):
    def test_a_clean_detach_adds_only_the_window(self):
        closed = proof.mapped_surfaces(CLOSED)
        detached = proof.mapped_surfaces(CLOSED + OPEN_PANEL + PREPARED + UNMAP_PANEL)
        self.assertEqual(proof.surface_delta(closed, detached), {'added': ['30:xdg'], 'removed': []})

    def test_a_panel_layer_left_mapped_is_reported(self):
        closed = proof.mapped_surfaces(CLOSED)
        ghost = proof.mapped_surfaces(CLOSED + OPEN_PANEL + PREPARED)
        self.assertEqual(proof.surface_delta(closed, ghost)['added'], ['20:layer:bead-dev', '30:xdg'])

    def test_subsurfaces_are_not_role_surfaces(self):
        trace = CLOSED + '[7.0]  -> wl_subcompositor#5.get_subsurface(new id wl_subsurface#40, wl_surface#41, wl_surface#10)\n'
        self.assertEqual(proof.surface_delta({}, {'41': 'subsurface', **proof.mapped_surfaces(trace)}), {'added': ['10:layer:bead-dev'], 'removed': []})

    def test_events_and_uncommitted_attaches_do_not_count(self):
        trace = CLOSED + '[5.0] wl_surface#10.enter(wl_output#9)\n[5.1]  -> wl_surface#10.attach(nil, 0, 0)\n'
        self.assertEqual(proof.mapped_surfaces(trace), {'10': 'layer:bead-dev'})

    def test_destroyed_and_reused_ids_start_unmapped(self):
        trace = CLOSED + '[6.0]  -> wl_surface#10.destroy()\n[6.1]  -> wl_compositor#4.create_surface(new id wl_surface#10)\n'
        self.assertEqual(proof.mapped_surfaces(trace), {})


if __name__ == '__main__':
    unittest.main()
