import argparse
import asyncio
import json
import time
from pathlib import Path
from mcp_client import Client

parser = argparse.ArgumentParser(description='run_command의 빠른 handle과 실제 Windows progress stream을 검증해')
parser.add_argument('--binary', required=True, type=Path)
parser.add_argument('--pairing', required=True, type=Path)
parser.add_argument('--project-id', required=True)
parser.add_argument('--evidence', required=True, type=Path)
args = parser.parse_args()

async def main():
    args.evidence.mkdir(parents=True, exist_ok=True)
    log_path = args.evidence / 'streaming-mcp.jsonl'
    with log_path.open('w') as log, (args.evidence / 'streaming-stderr.log').open('w') as stderr:
        process = await asyncio.create_subprocess_exec(str(args.binary.resolve()), 'hub', '--config', str(args.pairing.resolve()), stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=stderr, limit=16*1024*1024)
        client = Client(process, log, args.project_id)
        receiver = asyncio.create_task(client.receive())
        try:
            await client.request('initialize', {'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'streaming-verification','version':'1'}})
            await client.notify('notifications/initialized')
            began = time.monotonic()
            first = (await client.tool('run_command',command='stream'))['structuredContent']
            first_ms = (time.monotonic()-began)*1000
            assert first['status'] == 'running', first
            cursor = first['next_cursor']
            text = ''.join(event['text'] for event in first['events'])
            while True:
                output = (await client.tool('read_process_output',process_id=first['process_id'],cursor=cursor,wait_seconds=30))['structuredContent']
                text += ''.join(event['text'] for event in output['events'])
                cursor = output['next_cursor']
                if output['status'] == 'stopped':
                    assert output['result']['success'] and output['result']['exit_code'] == 0
                    assert output['result']['revision_verified']
                    break
            for marker in ['STDOUT_1','STDOUT_2','STDERR_1','STDERR_2']:
                assert marker in text, text
            default = {'process_id':first['process_id'],'first_response_ms':first_ms,'completion_ms':(time.monotonic()-began)*1000,'result':output['result']}
            print('RUN_COMMAND_YIELDED', json.dumps(default), flush=True)

            async def waiting(command, token):
                began = time.monotonic()
                request_id = client.next_id+1
                result = await client.request('tools/call', {'name':'run_command','arguments':{'project_id':args.project_id,'command':command,'wait':True},'_meta':{'progressToken':token}})
                assert result.get('isError') is not True, result
                assert result['structuredContent']['exit_code'] == 0
                return {'request_id':request_id,'started':began,'finished':time.monotonic(),'result':result['structuredContent']}

            streamed, parallel = await asyncio.gather(waiting('stream','native-stream'), waiting('build','parallel-build'))
            packets = [json.loads(line) for line in log_path.read_text().splitlines()]
            samples = {}
            for token, completed in [('native-stream',streamed),('parallel-build',parallel)]:
                notifications = [packet for packet in packets if packet['message'].get('method') == 'notifications/progress' and packet['message']['params']['progressToken'] == token]
                assert notifications, token
                progress = 0
                for packet in notifications:
                    params = packet['message']['params']
                    assert params['progress'] > progress
                    progress = params['progress']
                    data = params['_meta']['io.w7bridge/output']
                    assert data['project_id'] == args.project_id
                    assert data['process_id'] == completed['result']['process_id']
                samples[token] = {'count':len(notifications),'process_id':completed['result']['process_id'],'exit_code':completed['result']['exit_code']}
            early = [packet for packet in packets if packet['message'].get('method') == 'notifications/progress' and packet['message']['params']['progressToken'] == 'native-stream']
            streams = {'stdout':'','stderr':''}
            arrivals = {}
            for packet in early:
                data = packet['message']['params']['_meta']['io.w7bridge/output']
                for event in data['events']:
                    streams[event['stream']] += event['text']
                for stream, marker in [('stdout','STDOUT_1'),('stderr','STDERR_1')]:
                    if marker in streams[stream] and stream not in arrivals:
                        arrivals[stream] = packet['received_monotonic']
            assert set(arrivals) == {'stdout','stderr'}
            for arrival in arrivals.values():
                assert streamed['finished']-arrival > 1, '출력이 종료 전에 도착하지 않았습니다'
            for stream, marker in [('stdout','STDOUT_2'),('stderr','STDERR_2')]:
                assert marker in streams[stream]
            result = {'default':default,'notification_routes':samples,'stdout_before_completion_ms':(streamed['finished']-arrivals['stdout'])*1000,'stderr_before_completion_ms':(streamed['finished']-arrivals['stderr'])*1000,'stream_result':streamed['result'],'parallel_result':parallel['result']}
            (args.evidence/'streaming-result.json').write_text(json.dumps(result,indent=2)+'\n')
            print('NATIVE_STREAMING_VERIFIED',json.dumps({key:result[key] for key in ['notification_routes','stdout_before_completion_ms','stderr_before_completion_ms']}),flush=True)
        finally:
            process.stdin.close()
            try:
                await asyncio.wait_for(process.wait(),15)
            except asyncio.TimeoutError:
                process.terminate()
                await process.wait()
            await receiver

asyncio.run(main())
