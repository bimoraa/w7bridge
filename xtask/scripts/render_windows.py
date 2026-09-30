import argparse
import json
import urllib.request
import webbrowser
from pathlib import Path

parser = argparse.ArgumentParser(description='검증 app의 실제 HTTP 응답을 확인하고 Windows browser를 열어')
parser.add_argument('evidence', type=Path)
args = parser.parse_args()
body = urllib.request.urlopen('http://127.0.0.1:18758',timeout=10).read()
assert b'mac-edit-v2' in body
args.evidence.mkdir(exist_ok=True)
(args.evidence / 'render.html').write_bytes(body)
assert webbrowser.open('http://127.0.0.1:18758')
(args.evidence / 'browser-ready.json').write_text(json.dumps({'url':'http://127.0.0.1:18758','compiled_revision':'mac-edit-v2'}))
