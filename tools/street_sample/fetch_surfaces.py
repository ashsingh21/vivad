#!/usr/bin/env python3
"""Fetch three small CC0 PBR surface sets from Poly Haven for the street sample.

This preparation step is optional; the scene builder also has original procedural fallbacks.
Runtime loading never contacts the network. Powered by Poly Haven: https://polyhaven.com
"""
import hashlib
import argparse
import json
import time
import urllib.error
from pathlib import Path
import urllib.parse
import urllib.request

from PIL import Image

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'assets/models/samples/delhi_street/textures/vendor'
ASSETS={'asphalt':'asphalt_02','plaster':'painted_plaster_wall','concrete':'concrete_floor'}
ALLOWED={'api.polyhaven.com','dl.polyhaven.org','cdn.polyhaven.com'}


def fetch(url):
    assert urllib.parse.urlparse(url).scheme=='https'
    assert urllib.parse.urlparse(url).hostname in ALLOWED
    request=urllib.request.Request(url,headers={'User-Agent':'Vivad Delhi street sample / Powered by Poly Haven'})
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request,timeout=45) as response:
                assert urllib.parse.urlparse(response.geturl()).hostname in ALLOWED
                return response.read()
        except urllib.error.HTTPError as error:
            if error.code<500 or attempt==2:raise
            time.sleep(.5*(attempt+1))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inspect',action='store_true',help='Inspect public material metadata without downloading maps')
    args=parser.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    manifest={'license':'CC0-1.0','license_url':'https://polyhaven.com/license','api_credit':'Powered by Poly Haven','assets':{}}
    for local,asset in ASSETS.items():
        data=json.loads(fetch('https://api.polyhaven.com/files/'+asset))
        channels={key.lower():value for key,value in data.items()}
        if args.inspect:
            print(asset,list(data.keys()))
            continue
        item={'asset_id':asset,'source':'https://polyhaven.com/a/'+asset,'maps':{}}
        for channel in ['diff','nor_gl','arm' if 'arm' in channels else 'rough']:
            variants=channels.get(channel) or channels.get('diffuse' if channel=='diff' else channel)
            options=variants['1k']
            # The current Bevy build supports PNG but does not enable its optional JPEG decoder.
            selected=options.get('png') or options.get('jpg')
            assert selected and selected.get('url'),(asset,channel)
            url=selected['url']
            name=Path(urllib.parse.urlparse(url).path).name
            path=OUT/name
            if not path.exists():
                payload=fetch(url)
                pending=path.with_suffix(path.suffix+'.download')
                pending.write_bytes(payload)
                with Image.open(pending) as image:image.verify()
                pending.replace(path)
            item['maps'][channel]={'file':name,'url':url,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
        manifest['assets'][local]=item
        (OUT/'sources.json').write_text(json.dumps(manifest,indent=2)+'\n')
        print(local,asset,'1k maps downloaded')
    if not args.inspect:(OUT/'sources.json').write_text(json.dumps(manifest,indent=2)+'\n')


if __name__=='__main__':main()
