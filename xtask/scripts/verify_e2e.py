import argparse
import asyncio
import base64
import hashlib
import json
import time
from pathlib import Path
from mcp_client import Client

parser = argparse.ArgumentParser(description='Mac hub에서 sync → Windows build/test → log → screenshot을 검증해')
parser.add_argument('--binary', required=True, type=Path)
parser.add_argument('--pairing', required=True, type=Path)
parser.add_argument('--local-root', required=True, type=Path)
parser.add_argument('--project-id', required=True)
parser.add_argument('--evidence', required=True, type=Path)
parser.add_argument('--resume', action='store_true')
parser.add_argument('--revision', default='mac-edit-v2')
parser.add_argument('--fresh-service', action='store_true')
args = parser.parse_args()

async def main():
    args.evidence.mkdir(parents=True, exist_ok=True)
    with (args.evidence / 'e2e-mcp.jsonl').open('a' if args.resume else 'w') as log, (args.evidence / 'e2e-stderr.log').open('a' if args.resume else 'w') as stderr:
        process = await asyncio.create_subprocess_exec(str(args.binary.resolve()),'hub','--config',str(args.pairing.resolve()),stdin=asyncio.subprocess.PIPE,stdout=asyncio.subprocess.PIPE,stderr=stderr,limit=16*1024*1024)
        client = Client(process,log,args.project_id)
        receiver = asyncio.create_task(client.receive())
        try:
            await client.request('initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'w7bridge-e2e','version':'1'}})
            await client.notify('notifications/initialized')
            initial = args.local_root / 'revision.txt'
            deadline = time.monotonic() + 120
            while not initial.exists():
                assert time.monotonic() < deadline, 'Windows initial import가 완료되지 않았습니다'
                await asyncio.sleep(.5)
            if args.resume:
                assert initial.read_text() == args.revision+'\n'
            else:
                assert initial.read_text() == 'windows-initial-v1\n'
                initial.write_text(args.revision+'\n')
            verified = {}
            for name in ['check','build','test']:
                existing = (await client.tool('list_processes'))['structuredContent'] if args.resume else None
                handles = [item for item in existing['processes'] if item['command'] == name] if existing else []
                if args.resume and not args.fresh_service and name == 'check' and not handles:
                    raise RuntimeError('이전 요청의 handle을 찾을 수 없어 command를 재실행하지 않습니다')
                start = handles[-1] if handles else (await client.tool('start_process',command=name))['structuredContent']
                cursor = 0
                events = []
                while True:
                    output = (await client.tool('read_process_output',process_id=start['process_id'],cursor=cursor,wait_seconds=30))['structuredContent']
                    events += output['events']
                    cursor = output['next_cursor']
                    if output['status'] == 'stopped':
                        result = output['result']
                        assert result['success'] and result['exit_code'] == 0 and result['revision_verified'], result
                        assert result['revision_at_start'] == result['revision_at_completion'], result
                        verified[name] = {'process_id':start['process_id'],'result':result,'events':events}
                        break
            existing = (await client.tool('list_processes'))['structuredContent'] if args.resume else None
            handles = [item for item in existing['processes'] if item['command'] == 'run' and item['status'] == 'running'] if existing else []
            start = handles[-1] if handles else (await client.tool('start_process',command='run'))['structuredContent']
            output = (await client.tool('read_process_output',process_id=start['process_id'],wait_seconds=30))['structuredContent']
            deadline = time.monotonic() + 60
            while not {'stdout','stderr'}.issubset({event['stream'] for event in output['events']}):
                assert time.monotonic() < deadline, output
                await asyncio.sleep(.5)
                output = (await client.tool('read_process_output',process_id=start['process_id']))['structuredContent']
            assert args.revision in ''.join(event['text'] for event in output['events'])
            replay = (await client.tool('read_process_output',process_id=start['process_id'],cursor=output['next_cursor']))['structuredContent']
            assert not replay['events']
            verified['run'] = {'process_id':start['process_id'],'output':output}
            verified['status'] = (await client.tool('project_status'))['structuredContent']
            verified['events'] = (await client.tool('read_events'))['structuredContent']
            verified['revision_file_sha256'] = hashlib.sha256(initial.read_bytes()).hexdigest()
            (args.evidence / 'e2e-result.json').write_text(json.dumps(verified,indent=2))
            print('E2E_BUILD_TEST_LOG_VERIFIED',flush=True)
            marker = args.evidence / 'browser-ready'
            deadline = time.monotonic() + 120
            while not marker.exists():
                assert time.monotonic() < deadline, 'Windows browser fixture가 준비되지 않았습니다'
                await asyncio.sleep(.5)
            screenshot = await client.tool('capture_screenshot')
            images = [content for content in screenshot['content'] if content['type'] == 'image']
            assert images and images[0]['mimeType'] == 'image/png'
            (args.evidence / 'windows-verified.png').write_bytes(base64.b64decode(images[0]['data']))
            print('E2E_SCREENSHOT_SAVED',flush=True)
            await client.tool('stop_process',process_id=start['process_id'])
        finally:
            process.stdin.close()
            try:
                await asyncio.wait_for(process.wait(),15)
            except asyncio.TimeoutError:
                process.terminate()
                await process.wait()
            await receiver

asyncio.run(main())
