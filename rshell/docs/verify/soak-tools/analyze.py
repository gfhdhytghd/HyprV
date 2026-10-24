#!/usr/bin/env python3
"""Summarize raw soak observations; supports partial runs without claiming a pass."""
import argparse, collections, json, math, statistics, os, bisect
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]

def quantile(values,q):
    if not values:return None
    values=sorted(values);index=(len(values)-1)*q;lo=int(index);hi=math.ceil(index)
    return values[lo]+(values[hi]-values[lo])*(index-lo)

def regression(points):
    if len(points)<3:return {}
    xs,ys=zip(*points);mx=statistics.mean(xs);my=statistics.mean(ys)
    dx=sum((x-mx)**2 for x in xs);dy=sum((y-my)**2 for y in ys);cross=sum((x-mx)*(y-my) for x,y in points)
    return dict(slope_per_hour=cross/dx*3600 if dx else 0,pearson_r=cross/math.sqrt(dx*dy) if dx*dy else 0,first=ys[0],last=ys[-1],minimum=min(ys),maximum=max(ys),samples=len(points),span_s=max(xs)-min(xs),nondecreasing=all(b>=a for a,b in zip(ys,ys[1:])))

def readlines(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]

def analyze(run):
    status=json.loads((run/'status.json').read_text());samples=readlines(run/'samples.jsonl');events=readlines(run/'events.jsonl')
    frames=[];timer_calls=collections.Counter();errors=[];start=status['start_monotonic']
    for line in (run/'trace.log').read_text().splitlines():
        parts=line.split()
        if len(parts)!=6:continue
        kind,at,tid,surface,duration,detail=parts
        elapsed=int(at)/1e9-start
        if kind=='F':frames.append(dict(elapsed=elapsed,render_ms=int(duration)/1e6,swap_ms=int(detail)/1e6,surface=surface))
        elif kind=='E':errors.append(line)
        elif kind in ('T','C'):timer_calls[kind]+=1
    bins=[]
    for lo in range(0,math.ceil(status.get('elapsed',0)/300)*300,300):
        ss=[s for s in samples if lo<=s['elapsed']<lo+300]
        ff=[f for f in frames if lo<=f['elapsed']<lo+300]
        if not ss:continue
        bins.append(dict(start_s=lo,end_s=min(lo+300,status.get('elapsed',0)),rss_mib=round(statistics.median(s['shell']['rss_kib'] for s in ss)/1024,2),pss_mib=round(statistics.median(s['shell']['pss_kib'] for s in ss)/1024,2),fds_min=min(s['shell']['fds'] for s in ss),fds_max=max(s['shell']['fds'] for s in ss),threads_min=min(s['shell']['threads'] for s in ss),threads_max=max(s['shell']['threads'] for s in ss),armed_timers_min=min(s['shell']['armed_timerfds'] for s in ss),armed_timers_max=max(s['shell']['armed_timerfds'] for s in ss),frames=len(ff),render_p50_ms=quantile([f['render_ms'] for f in ff],.5),render_p99_ms=quantile([f['render_ms'] for f in ff],.99),ipc_p99_ms=quantile([s['ipc_ms'] for s in ss if 'ipc_ms' in s],.99)))
    extra=readlines(run/'extra-input.jsonl') if (run/'extra-input.jsonl').exists() else []
    supplement_start=next((e['elapsed'] for e in extra if e['name']=='supplement-start'),None)
    # Keep all raw data, but compare a stable workload after at least one full
    # supplementary cycle. The original clock hover intentionally stays compact.
    cohort_start=math.ceil((supplement_start+180)/180)*180 if supplement_start else 180
    settled=[s for s in samples if s['elapsed']>=cohort_start and 5<=s['elapsed']%180<18]
    trends={key:regression([(s['elapsed'],s['shell'][key]) for s in settled]) for key in ('rss_kib','pss_kib','private_dirty_kib','fds','threads','armed_timerfds')}
    # In-flight sockets/fences can live for a single sample. Persistent count
    # leaks raise the per-cycle quiet floor; preserve all raw maxima separately.
    for key in ('fds','threads','armed_timerfds'):
        cycles=collections.defaultdict(list)
        for sample in settled:cycles[int(sample['elapsed']//180)].append(sample)
        trends[key]=regression([(statistics.mean(s['elapsed'] for s in rows),min(s['shell'][key] for s in rows)) for _,rows in sorted(cycles.items())])
    gpu=[]
    for s in samples:
        for row in s.get('gpu',{}).get('rows',[]):
            try:memory=float(row['memory'].split()[0])
            except (ValueError,AttributeError):continue
            gpu.append(dict(elapsed=s['elapsed'],memory_mib=memory))
    trends['gpu_mib']=regression([(s['elapsed'],s['memory_mib']) for s in gpu if s['elapsed']>=cohort_start and s['elapsed']%180<20])
    warm=[f for f in frames if f['elapsed']>=120]
    flags=[]
    for key,trend in trends.items():
        if not trend:continue
        threshold={'rss_kib':10240,'pss_kib':10240,'private_dirty_kib':10240,'fds':4,'threads':1,'armed_timerfds':1,'gpu_mib':16}[key]
        if trend['span_s']>=900 and trend['slope_per_hour']>threshold and trend['pearson_r']>.8:flags.append(dict(kind='growth-screen',metric=key,**trend))
        elif trend['span_s']>=1800 and trend['nondecreasing'] and trend['last']-trend['first']>({'rss_kib':64,'pss_kib':64,'private_dirty_kib':64}.get(key,0)):
            flags.append(dict(kind='small-monotonic-retention-observation',metric=key,**trend))
    probes=[dict(elapsed=s['elapsed'],error=s['probe_error']) for s in samples if 'probe_error' in s]
    failed_events=[e for e in events if not e.get('ok')]
    slow_ipc=[dict(elapsed=s['elapsed'],milliseconds=s['ipc_ms']) for s in samples if s.get('ipc_ms',0)>500]
    for item in probes+failed_events+slow_ipc:flags.append(dict(kind='driver-or-liveness',observation=item))
    frame_times=sorted(f['elapsed'] for f in frames)
    response=[]
    visual={'calendar-open','calendar-close','calendar-over-media','calendar-over-media-close','control-open','volume','brightness-preview','agent-processing','agent-waiting','agent-completed','media-play','media-pause','agent-side-hover','media-side-hover','stats-click'}
    for event in events+extra:
        if event.get('name') not in visual or 'monotonic_ns' not in event:continue
        at=event['monotonic_ns']/1e9-start;index=bisect.bisect_left(frame_times,at)
        if index<len(frame_times):response.append(dict(name=event['name'],elapsed=at,next_swap_ms=(frame_times[index]-at)*1000))
    slow_response=[event for event in response if event['next_swap_ms']>500]
    slow_render=[f for f in warm if f['render_ms']>250]
    for event in slow_response:flags.append(dict(kind='slow-next-swap-proxy',observation=event))
    first=samples[0]['shell'];last=samples[-1]['shell']
    report=dict(status=status,samples=len(samples),events=len(events),event_counts=dict(collections.Counter(e['name'] for e in events)),supplement_event_counts=dict(collections.Counter(e['name'] for e in extra)),supplement_expansions_verified=sum(e.get('expanded_verified') is True for e in extra),supplement_stats_verified=sum(e['name']=='stats-click' and e.get('stats_mapped') is True for e in extra),cohort_start_s=cohort_start,failed_events=failed_events,probe_errors=probes,egl_errors=errors,frame_count=len(frames),post_warmup_frame_count=len(warm),render_p50_ms=quantile([f['render_ms'] for f in warm],.5),render_p99_ms=quantile([f['render_ms'] for f in warm],.99),render_max_ms=max((f['render_ms'] for f in warm),default=0),swap_p99_ms=quantile([f['swap_ms'] for f in warm],.99),ipc_p50_ms=quantile([s['ipc_ms'] for s in samples if 'ipc_ms' in s],.5),ipc_p99_ms=quantile([s['ipc_ms'] for s in samples if 'ipc_ms' in s],.99),first=first,last=last,bins=bins,trends=trends,gpu_samples=gpu,flags=flags,timer_interposer_counts=dict(timer_calls),cpu_percent=(last['cpu_ticks']-first['cpu_ticks'])/os.sysconf('SC_CLK_TCK')/(samples[-1]['elapsed']-samples[0]['elapsed'])*100 if len(samples)>1 else 0,coverage_note='No live providers. Only actual nested compositor outputs, private mock MPRIS/logind and synthetic agent hooks. Internal Tokio timer cardinality and GPU execution latency are not exposed.')
    report.update(next_swap_proxy_p50_ms=quantile([e['next_swap_ms'] for e in response],.5),next_swap_proxy_p99_ms=quantile([e['next_swap_ms'] for e in response],.99),next_swap_proxy_max_ms=max((e['next_swap_ms'] for e in response),default=0),slow_next_swap_proxy=slow_response,render_over_250ms=slow_render,next_swap_proxy_note='First observed swap after an event, across surfaces; not proof that that specific update was presented.')
    (run/'analysis.json').write_text(json.dumps(report,indent=2)+'\n')
    return report,samples

def chart(path,samples):
    width,height=1040,440;left,top,right,bottom=70,35,40,55;usable_w=width-left-right;usable_h=height-top-bottom
    end=max(s['elapsed'] for s in samples);maximum=max(s['shell']['rss_kib']/1024 for s in samples)*1.1
    def line(key,color):
        points=' '.join(f"{left+s['elapsed']/max(end,1)*usable_w:.1f},{top+usable_h-s['shell'][key]/1024/maximum*usable_h:.1f}" for s in samples)
        return f'<polyline points="{points}" fill="none" stroke="{color}" stroke-width="2"/>'
    content=[f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="100%" height="100%" fill="#101820"/><g fill="#d9e4ef" font-family="sans-serif" font-size="13"><text x="70" y="22">Release shell RSS / PSS (MiB), isolated activity soak</text>']
    for i in range(6):
        y=top+usable_h-i*usable_h/5
        content.append(f'<path d="M {left} {y} H {width-right}" stroke="#304050"/><text x="8" y="{y+4}">{maximum*i/5:.0f}</text>')
    for minute in range(0,int(end/60)+1,10):
        x=left+minute*60/max(end,1)*usable_w
        content.append(f'<text x="{x-8}" y="{height-26}">{minute}</text>')
    content.extend([f'<text x="{width/2}" y="{height-6}">Elapsed minutes</text><text x="770" y="22" fill="#62c2ff">RSS</text><text x="850" y="22" fill="#f5c25e">PSS</text></g>',line('rss_kib','#62c2ff'),line('pss_kib','#f5c25e'),'</svg>'])
    path.write_text(''.join(content))

def gpu_chart(path,report):
    rows=report['gpu_samples']
    if not rows:return
    width,height=1040,360;left,top=65,45;w,h=930,260
    end=max(r['elapsed'] for r in rows);maximum=math.ceil(max(r['memory_mib'] for r in rows)/10)*10
    points=' '.join(f"{left+r['elapsed']/end*w:.1f},{top+h-r['memory_mib']/maximum*h:.1f}" for r in rows)
    parts=[f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"><rect width="100%" height="100%" fill="#101820"/><g font-family="sans-serif" font-size="13" fill="#d9e4ef"><text x="65" y="22">NVML process graphics memory (MiB); includes retained/reserved driver/cache memory</text>']
    for value in range(0,maximum+1,10):
        y=top+h-value/maximum*h
        parts.append(f'<path d="M {left} {y} H {left+w}" stroke="#304050"/><text x="12" y="{y+4}">{value}</text>')
    for minute in range(0,int(end/60)+1,10):
        x=left+minute*60/end*w
        parts.append(f'<text x="{x-6}" y="{height-30}">{minute}</text>')
    parts.append(f'<text x="480" y="{height-5}">Elapsed minutes</text></g><polyline points="{points}" fill="none" stroke="#62c2ff" stroke-width="1.8"/>')
    for row in rows:
        if row['elapsed']>=report['cohort_start_s'] and row['elapsed']%180<20:
            x=left+row['elapsed']/end*w;y=top+h-row['memory_mib']/maximum*h
            parts.append(f'<circle cx="{x}" cy="{y}" r="3.5" fill="#f5c25e"/>')
    parts.append('<text x="710" y="42" fill="#f5c25e" font-family="sans-serif" font-size="12">Gold: matching settled cycle phase</text></svg>')
    path.write_text(''.join(parts))

def main():
    p=argparse.ArgumentParser();p.add_argument('run',type=Path);args=p.parse_args();report,samples=analyze(args.run);chart(args.run/'memory.svg',samples);gpu_chart(args.run/'gpu.svg',report)
    print(json.dumps({key:report[key] for key in ('samples','events','frame_count','render_p50_ms','render_p99_ms','ipc_p99_ms','trends','flags')},indent=2))

if __name__=='__main__':main()
