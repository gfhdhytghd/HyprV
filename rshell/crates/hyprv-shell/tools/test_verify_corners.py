"""Unmasked corner acceptance must not inherit the full-image text tolerance."""
import json
from pathlib import Path
import tempfile
import unittest

from PIL import Image
from verify_corners import verify


class Corners(unittest.TestCase):
    def test_each_corner_is_scored_and_a_single_alpha_level_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            job = run / 'idle-dark'
            job.mkdir()
            reference = run / 'qt.png'
            actual = job / 'screen.png'
            image = Image.new('RGBA', (200, 200), (0, 0, 0, 0))
            image.save(reference)
            image.save(actual)
            (job / 'manifest.json').write_text(json.dumps(dict(
                capture_manifest='manifest.json', captures=[dict(
                    file='screen.png', output='DP-test', scale=2,
                    comparison=dict(reference=str(reference)))])))
            self.assertTrue(verify(run)['pass'])
            for point, corner in [((0, 0), 'top-left'), ((199, 0), 'top-right'),
                                  ((0, 199), 'bottom-left'), ((199, 199), 'bottom-right')]:
                changed = image.copy()
                changed.putpixel(point, (0, 0, 0, 1))
                changed.save(actual)
                result = verify(run)
                self.assertFalse(result['pass'])
                self.assertEqual(result['masked_pixels'], 0)
                failed = [r for r in result['rows'] if r['max_channel_diff']]
                self.assertEqual([(r['corner'], r['different_pixels']) for r in failed], [(corner, 1)])

    def test_missing_captures_cannot_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, 'no assembled captures'):
                verify(Path(tmp))


if __name__ == '__main__':
    unittest.main()
