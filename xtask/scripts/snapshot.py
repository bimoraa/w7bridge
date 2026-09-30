import argparse
import hashlib
import io
import json
import tarfile
from pathlib import Path

parser = argparse.ArgumentParser(description='native 검증에 필요한 source와 SHA-256 manifest를 묶어')
parser.add_argument('--output', required=True, type=Path)
parser.add_argument('--manifest', required=True, type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
files = [root / name for name in ['Cargo.toml', 'Cargo.lock', 'rustfmt.toml', 'w7bridge.example.toml'] if (root / name).is_file()]
files += sorted(path for name in ['src', 'tests', 'xtask'] for path in (root / name).rglob('*')
                if path.is_file() and not set(path.relative_to(root).parts) & {'target', '__pycache__'})
records = [{'path': str(path.relative_to(root)), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in files]
source_id = hashlib.sha256(json.dumps(records, sort_keys=True).encode()).hexdigest()
manifest = json.dumps({'source_id': source_id, 'files': records}, indent=2).encode()
args.manifest.write_bytes(manifest)
with tarfile.open(args.output, 'w:gz') as archive:
    for path in files:
        archive.add(path, arcname=str(path.relative_to(root)))
    entry = tarfile.TarInfo('source-manifest.json')
    entry.size = len(manifest)
    archive.addfile(entry, io.BytesIO(manifest))
print(source_id)
