#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Bake Welcome's offline demo through Nacre's existing Colors JSON-lines helper."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from PIL import Image, ImageOps


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helper', required=True, type=Path)
    parser.add_argument('--assets', required=True, type=Path, help='Nacre shell-tools directory')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    helper, assets = args.helper.resolve(), args.assets.resolve()
    manifest = json.loads((assets / 'demo-wallpapers/manifest.json').read_text())
    output = args.output
    output.mkdir(parents=True, exist_ok=True)
    scenes = []
    with tempfile.TemporaryDirectory(prefix='welcome-bake-') as private:
        env = dict(os.environ, HOME=private, NACRE_HELPER_TOOLS=str(assets),
                   XDG_CONFIG_HOME=private+'/config', XDG_STATE_HOME=private+'/state',
                   XDG_CACHE_HOME=private+'/cache', XDG_DATA_HOME=private+'/data')
        for item in sorted(manifest['wallpapers'], key=lambda item: item['file']):
            source = assets / 'demo-wallpapers' / item['file']
            if hashlib.sha256(source.read_bytes()).hexdigest() != item['sha256']:
                raise ValueError('bundled wallpaper checksum mismatch')
            scene = {key: item[key] for key in ('name', 'artist', 'source', 'license', 'sha256')}
            scene['image'] = item['file']
            scene['thumbnail'] = source.stem+'-thumb.jpg'
            scene['palettes'] = {}
            with Image.open(source) as image:
                for name, size in [(scene['image'], (1280, 720)), (scene['thumbnail'], (512, 288))]:
                    ImageOps.fit(image.convert('RGB'), size, Image.Resampling.LANCZOS).save(
                        output / name, quality=90, optimize=True)
            for mode in ('dark', 'light'):
                request = {'image': str(source), 'mode': mode, 'personality': 'natural',
                           'backgroundFromWallpaper': False, 'hour': 12, 'compare': True}
                result = subprocess.run([str(helper), 'colors', 'preview'], env=env,
                                        input=json.dumps(request)+'\n', capture_output=True,
                                        text=True, check=True)
                preview = json.loads(result.stdout)
                scene['palettes'][mode] = {row['personality']: row['colours']
                                           for row in preview['comparisons']}
                if len(scene['palettes'][mode]) != 8:
                    raise ValueError('helper did not return all eight styles')
                print(source.stem, mode, flush=True)
            scenes.append(scene)
    (output / 'palettes.json').write_text(json.dumps({'version': 1, 'helperSha256': hashlib.sha256(helper.read_bytes()).hexdigest(),
                                                     'changes': 'Resized and cropped to 1280x720 posters and 512x288 thumbnails.',
                                                     'scenes': scenes},
                                                    ensure_ascii=False, separators=(',', ':'))+'\n')


if __name__ == '__main__':
    main()
