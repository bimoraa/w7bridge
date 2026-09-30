import argparse
import json
import os
import shutil
import subprocess
import sys
import traceback
import time
from pathlib import Path

parser = argparse.ArgumentParser(description='임시 Windows service/build fixture를 준비해')
parser.add_argument('root', type=Path)
parser.add_argument('--stop', action='store_true')
parser.add_argument('--reinstall', action='store_true')
args = parser.parse_args()
root = args.root.resolve()
log = (root / 'fixture-bootstrap.log').open('w', encoding='utf-8', buffering=1)
sys.stdout = log
sys.stderr = log
def run(command, **options):
    if not options.get("capture_output"):
        options.setdefault("stdout", log)
        options.setdefault("stderr", log)
    return subprocess.run(command, **options)
def report_error(kind, value, trace):
    log.write(''.join(traceback.format_exception(kind, value, trace)))
sys.excepthook = report_error
print('FIXTURE_BEGIN', flush=True)
fixture = root / 'e2e'
if args.stop:
    current = run(['sc.exe','qc','w7bridge'],capture_output=True,text=True)
    assert current.returncode == 0 and str(root).lower() in current.stdout.lower() and 'service.toml' in current.stdout, '소유 service만 중지합니다'
    run(['sc.exe','stop','w7bridge'],check=True)
    deadline = time.monotonic()+30
    while 'STOPPED' not in run(['sc.exe','query','w7bridge'],capture_output=True,text=True).stdout:
        assert time.monotonic() < deadline
        time.sleep(.2)
    run(['sc.exe','delete','w7bridge'],check=True)
    print('OWNED_SERVICE_REMOVED',flush=True)
    raise SystemExit(0)
if args.reinstall:
    assert fixture.is_dir() and (fixture/'service.toml').is_file()
    assert run(['sc.exe','query','w7bridge'],capture_output=True).returncode == 1060
    installed=fixture/'bridge'
    installed.mkdir(exist_ok=True)
    binary=installed/'w7bridge.exe'
    shutil.copy2(root/'target/debug/w7bridge.exe',binary)
    run([str(binary),'service','install','--config',str(fixture/'service.toml')],check=True)
    print('OWNED_SERVICE_REINSTALLED',flush=True)
    raise SystemExit(0)
assert not fixture.exists(), '기존 fixture를 보존합니다' 
fixture.mkdir()
project = fixture / 'project'
(project / 'src').mkdir(parents=True)
(project / 'Cargo.toml').write_text('[package]\nname="w7bridge_e2e_fixture"\nversion="0.1.0"\nedition="2024"\n\n[workspace]\n')
(project / 'Cargo.lock').write_text('version = 4\n\n[[package]]\nname = "w7bridge_e2e_fixture"\nversion = "0.1.0"\n')
(project / 'revision.txt').write_text('windows-initial-v1\n')
(project / 'MEMORY.md').write_text('Windows-only initial context\n')
(project / 'src/main.rs').write_text(r'''use std::{io::{Read,Write},net::TcpListener};
const revision: &str = include_str!("../revision.txt");
fn main() {
    println!("APP_STDOUT revision={}",revision.trim());
    eprintln!("APP_STDERR revision={}",revision.trim());
    let listener = TcpListener::bind("127.0.0.1:18758").unwrap();
    for incoming in listener.incoming() {
        let mut connection = incoming.unwrap();
        let mut request = [0;4096];
        let _ = connection.read(&mut request);
        let body = format!("<!doctype html><html><body style='background:#111;color:#fff;font:32px sans-serif;padding:50px'><h1>w7bridge native Windows verification</h1><p>Compiled revision: <strong>{}</strong></p><p>Mac edit → sync → Windows Cargo build/test → live output</p></body></html>",revision.trim());
        write!(connection,"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    }
}
#[test]
fn mac_revision_was_compiled() { assert_eq!(revision,"mac-edit-v2\n"); }
''', encoding='utf-8')
git = 'C:/Program Files/Git/cmd/git.exe'
for command in [['init','--quiet','--initial-branch=fixture'], ['add','.'], ['-c','user.name=fixture','-c','user.email=fixture@example.invalid','commit','--quiet','-m','fixture initial revision']]:
    run([git] + command, cwd=project, check=True)
source = Path.home() / '.rustup/toolchains/1.97.1-x86_64-pc-windows-msvc'
toolchain = fixture / 'toolchain'
shutil.copytree(source, toolchain)
(fixture / 'cargo-home').mkdir()
vs = Path('C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC')
vc = sorted(vs.iterdir())[-1]
kits = Path('C:/Program Files (x86)/Windows Kits/10')
version = sorted((kits / 'Lib').iterdir())[-1].name
env = {
    'CARGO_HOME':str(fixture / 'cargo-home'), 'RUSTC':str(toolchain / 'bin/rustc.exe'),
    'PATH':';'.join(str(path) for path in [toolchain / 'bin',vc / 'bin/Hostx64/x64',kits / 'bin' / version / 'x64']) + ';C:/Windows/System32;C:/Windows',
    'LIB':';'.join(str(path) for path in [vc / 'lib/x64',kits / 'Lib' / version / 'ucrt/x64',kits / 'Lib' / version / 'um/x64']),
    'INCLUDE':';'.join(str(path) for path in [vc / 'include']+[kits / 'Include' / version / name for name in ['ucrt','shared','um','winrt']]),
}
lines = ['version = 1','device_id = "windows-2158"','[codex]','enabled = false','[execution]','timeout_seconds = 60','concurrency = 3','output_bytes = 262144','[service]','allowed_sid = "S-1-5-21-4267122284-3017888972-3023285092-1001"','[[projects]]','id = "fixture"',f'root = {json.dumps(str(project))}','requires_sync = true','[projects.files]','enabled = true','[projects.git]',f'executable = {json.dumps(git)}']
for name,arguments in [('check',['check','--locked']),('build',['build','--locked']),('test',['test','--locked','--','--nocapture']),('run',['run','--locked'])]:
    lines += [f'[projects.commands.{name}]',f'executable = {json.dumps(str(toolchain / "bin/cargo.exe"))}',f'args = {json.dumps(arguments)}']
    if name == 'run':
        lines += ['background = true','restart_on_sync = true']
    else:
        lines += ['source_snapshot = true']
    lines += [f'[projects.commands.{name}.env]'] + [f'{key} = {json.dumps(value)}' for key,value in env.items()]
(fixture / 'service.toml').write_text('\n'.join(lines)+'\n')
(fixture / 'capture.toml').write_text('version = 1\ndevice_id = "windows-2158"\nprojects = []\n[codex]\nenabled = false\n[screenshots]\nenabled = true\n')
run(['icacls.exe',str(fixture),'/grant','*S-1-5-19:(OI)(CI)M','/T','/Q'],check=True)
(fixture / 'fixture-ready.json').write_text(json.dumps({'project':str(project),'root':str(fixture),'toolchain':str(toolchain)},indent=2))
service = run(['sc.exe','query','w7bridge'],capture_output=True)
assert service.returncode == 1060, '기존 service를 변경하지 않습니다'
binary = root / 'target/debug/w7bridge.exe'
run([str(binary),'service','install','--config',str(fixture / 'service.toml')],check=True)
(fixture / 'service-ready.json').write_text(json.dumps({'binary':str(binary),'config':str(fixture / 'service.toml')}))

print('FIXTURE_END', flush=True)
