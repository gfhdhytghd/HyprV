"""Coverage regressions: absent renders must not silently pass the owner gate."""
from pathlib import Path
import json
import subprocess
import tempfile
import unittest
from PIL import Image


class Coverage(unittest.TestCase):
    def test_unexported_reference_counts_as_missing_and_empty_mask_is_valid(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            actual, reference, report = [root / name for name in ('actual', 'reference', 'report')]
            actual.mkdir()
            reference.mkdir()
            entry = dict(file='rendered.png', scale=1, foreign_island_local_rect=[0, 0, 0, 0])
            (actual / 'coverage.json').write_text(json.dumps(dict(exported=[entry])))
            (reference / 'manifest.json').write_text(json.dumps(dict(images=[
                dict(file=name, distance_field_text=False) for name in ('rendered.png', 'missing.png')
            ])))
            for path in (actual / 'rendered.png', reference / 'rendered.png', reference / 'missing.png'):
                Image.new('RGBA', (2, 2), (64, 80, 96, 255)).save(path)
            subprocess.run(['python3', str(Path(__file__).with_name('gate_metrics.py')),
                            str(actual), str(reference), str(report)], check=True, stdout=subprocess.DEVNULL)
            metrics = json.loads((report / 'gate.json').read_text())
            self.assertEqual((metrics['passed'], metrics['images'], metrics['missing']), (1, 2, 1))
            self.assertEqual(metrics['missing_files'], ['missing.png'])


if __name__ == '__main__':
    unittest.main()
