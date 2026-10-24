#!/usr/bin/env python3
"""Tests for arithmetic/identity boundaries used in the host reports."""
import importlib.util,pathlib,tempfile,unittest
spec=importlib.util.spec_from_file_location('verify',pathlib.Path(__file__).with_name('verify.py'));v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
class Metrics(unittest.TestCase):
    def snapshot(self,ticks,tasks):
        return {'processes':{str(pid):{'start':1,'ticks':n,'rss_kib':10}for pid,n in ticks.items()},'tasks':tasks}
    def test_reaped_child_cpu_does_not_count_prewindow_work(self):
        # Child had already used 80 ticks at start; parent later reaps all 100.
        before=self.snapshot({1:10,2:80},{})
        after=self.snapshot({1:120},{})
        self.assertAlmostEqual(v.delta(before,after,60)['cpu_seconds'],30/v.os.sysconf('SC_CLK_TCK'))
    def test_new_tasks_and_departed_tasks_are_explicit(self):
        keys=['runtime_ns','runqueue_ns','timeslices','voluntary','involuntary']
        before=self.snapshot({1:0},{'gone':dict.fromkeys(keys,2),'same':dict.fromkeys(keys,3)})
        after=self.snapshot({1:1},{'same':dict.fromkeys(keys,8),'new':dict.fromkeys(keys,4)})
        result=v.delta(before,after,10)
        self.assertEqual(result['task_counters']['timeslices'],9)
        self.assertEqual(result['exited_task_count'],1)
        self.assertEqual(result['new_task_count'],1)
    def test_partial_jsonl_does_not_fabricate_snapshot(self):
        # Own test artifact directory, never the system temp or provider state.
        directory=v.CRATE/'.check/tests';directory.mkdir(parents=True,exist_ok=True)
        path=directory/'partial.jsonl';path.write_text('{"topic":"x","state":false}\n{"topic":')
        self.assertEqual(v.states(path),[{'topic':'x','state':False}]);path.unlink()
class CalendarCorrelation(unittest.TestCase):
    def test_greeting_is_not_a_method_reply(self):
        import socket,threading,json
        directory=v.CRATE/'.check/tests';directory.mkdir(parents=True,exist_ok=True)
        path=directory/'rpc.sock'
        path.unlink(missing_ok=True)
        with socket.socket(socket.AF_UNIX) as server:
            server.bind(str(path));server.listen(1)
            def peer():
                with server.accept()[0] as channel:
                    request=json.loads(channel.makefile().readline())
                    channel.sendall((json.dumps({'apiVersion':1})+'\n'+json.dumps({'id':request['id'],'result':[]})+'\n').encode())
            worker=threading.Thread(target=peer);worker.start()
            try:self.assertEqual(v.rpc(str(path),'calendars.list',{}),{'id':1,'result':[]})
            finally:worker.join(timeout=2);path.unlink(missing_ok=True)
if __name__=='__main__':unittest.main()

