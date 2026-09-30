print('VERIFY_SCRIPT_BEGIN', flush=True)
import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
import traceback
import tarfile
from pathlib import Path

parser = argparse.ArgumentParser(description='Windows-native gate와 전체 로그를 기록해')
parser.add_argument('root', type=Path)
parser.add_argument('--archive', type=Path)
parser.add_argument('--previous-source')
parser.add_argument('--build-only', action='store_true')
args = parser.parse_args()
root = args.root.resolve()
def report_error(kind, value, trace):
    text = ''.join(traceback.format_exception(kind, value, trace))
    (root / 'bootstrap-error.log').write_text(text, encoding='utf-8')
    print(text, flush=True)
sys.excepthook = report_error
if args.archive:
    previous = json.loads((root / 'source-manifest.json').read_text())
    assert args.previous_source == previous['source_id'], '검증 root의 소유 snapshot이 달라졌습니다'
    for item in previous['files']:
        assert hashlib.sha256((root / item['path']).read_bytes()).hexdigest() == item['sha256'], item['path']
    if (root / 'windows-native.log').exists():
        (root / 'windows-native.log').replace(root / ('windows-native-' + previous['source_id'][:12] + '.log'))
    with tarfile.open(args.archive) as archive:
        archive.extractall(root, filter='data')
print('VERIFY_MANIFEST_BEGIN', flush=True)
manifest = json.loads((root / 'source-manifest.json').read_text())
for item in manifest['files']:
    assert hashlib.sha256((root / item['path']).read_bytes()).hexdigest() == item['sha256'], item['path']
print('VERIFY_MANIFEST_END', flush=True)
environment = os.environ.copy()
vs = Path('C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools')
vc = sorted((vs / 'VC/Tools/MSVC').iterdir())[-1]
kits = Path('C:/Program Files (x86)/Windows Kits/10')
kit_version = sorted((kits / 'Lib').iterdir())[-1].name
environment['LIB'] = ';'.join(str(path) for path in [vc / 'lib/x64', kits / 'Lib' / kit_version / 'ucrt/x64', kits / 'Lib' / kit_version / 'um/x64'])
environment['INCLUDE'] = ';'.join(str(path) for path in [vc / 'include'] + [kits / 'Include' / kit_version / name for name in ['ucrt','shared','um','winrt']])
environment['PATH'] = ';'.join(str(path) for path in [Path.home() / '.cargo/bin', vc / 'bin/Hostx64/x64', kits / 'bin' / kit_version / 'x64']) + ';' + environment['PATH']
environment['VCToolsInstallDir'] = str(vc) + '/'
environment['VSCMD_ARG_TGT_ARCH'] = 'x64'
environment['CARGO_TARGET_DIR'] = str(root / 'target')
commands = [['cargo','run','--locked','-p','xtask','--','--check'], ['cargo','check','--locked','--workspace','--all-targets'], ['cargo','clippy','--locked','--workspace','--all-targets','--','-D','warnings'], ['cargo','test','--locked','--workspace'], ['cargo','build','--locked']]
if args.build_only:
    commands = [["cargo","build","--locked"]]
results = []
with (root / 'windows-native.log').open('w', encoding='utf-8') as log:
    def emit(text):
        print(text, end='', flush=True)
        log.write(text)
        log.flush()
    emit('W7BRIDGE_NATIVE_BEGIN\n')
    for command in [['cargo','--version'], ['rustc','--version']] + commands:
        emit('COMMAND ' + ' '.join(command) + '\n')
        executable = Path.home() / '.cargo/bin' / (command[0] + '.exe')
        with subprocess.Popen([str(executable)] + command[1:], cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT) as child:
            for line in iter(child.stdout.readline, b''):
                emit(line.decode('utf-8', errors='replace'))
            code = child.wait()
        results.append({'command':command, 'exit_code':code})
        (root / 'progress.json').write_text(json.dumps(results, indent=2))
    result = {'source_id':manifest['source_id'], 'files_verified':len(manifest['files']), 'os':platform.platform(), 'commands':results}
    binary = root / 'target/debug/w7bridge.exe'
    if binary.is_file():
        result['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
    (root / 'complete.json').write_text(json.dumps(result, indent=2))
    emit('W7BRIDGE_NATIVE_END\n')
raise SystemExit(0 if len(results) == len(commands) + 2 and all(item['exit_code'] == 0 for item in results) else 1)
