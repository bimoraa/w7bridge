import asyncio
import json
import time

class Client:
    def __init__(self, process, log, project_id):
        self.process, self.log = process, log
        self.project_id = project_id
        self.pending = {}
        self.next_id = 0
    async def receive(self):
        while line := await self.process.stdout.readline():
            value = json.loads(line)
            self.log.write(json.dumps({'direction':'response','received_monotonic':time.monotonic(),'message':value})+'\n')
            self.log.flush()
            future = self.pending.pop(value.get('id'), None)
            if future and not future.done():
                future.set_result(value)
        for future in self.pending.values():
            if not future.done():
                future.set_exception(RuntimeError('MCP transport 종료'))
    async def notify(self, method, params=None):
        message = {'jsonrpc':'2.0','method':method}
        if params is not None:
            message['params'] = params
        self.process.stdin.write((json.dumps(message)+'\n').encode())
        await self.process.stdin.drain()
    async def request(self, method, params):
        self.next_id += 1
        future = asyncio.get_running_loop().create_future()
        self.pending[self.next_id] = future
        message = {'jsonrpc':'2.0','id':self.next_id,'method':method,'params':params}
        self.log.write(json.dumps({'direction':'request','message':message})+'\n')
        self.log.flush()
        self.process.stdin.write((json.dumps(message)+'\n').encode())
        await self.process.stdin.drain()
        response = await asyncio.wait_for(future, 150)
        assert 'error' not in response, response
        return response['result']
    async def tool(self, name, **arguments):
        deadline = time.monotonic() + 120
        while True:
            result = await self.request('tools/call', {'name':name,'arguments':{'project_id':self.project_id,**arguments}})
            if result.get('isError') is not True:
                return result
            if name not in {'read_process_output','list_processes','project_status','read_events'} or time.monotonic() >= deadline:
                raise RuntimeError(result)
            await asyncio.sleep(2)
