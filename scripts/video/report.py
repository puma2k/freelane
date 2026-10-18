"""Assess native probe evidence; never infer ARM/runtime/performance from compilation."""
import json
from pathlib import Path

WINDOWS_BOOLS=('capture_supported','owned_pixels_verified','low_latency_applied','zero_b_frames_applied','raw_packet_latency_measured')
MAC_BOOLS=('dimensions_verified','pixel_variation_verified','hardware_decoder_used','owned_surface_created','surface_rendering','onscreen_pixels_verified','copy_path_measured')

def validate(value):
    if not isinstance(value,dict) or type(value.get('schema')) is not int or value['schema']!=1:
        raise ValueError('Unsupported probe report schema')
    platform=value.get('platform')
    if platform not in ('windows','macos') or value.get('architecture') not in ('arm64','x86_64'):
        raise ValueError('Unsupported probe platform/architecture')
    bools=WINDOWS_BOOLS if platform=='windows' else MAC_BOOLS
    counts=('capture_frames','software_h264_count','hardware_h264_count','encoded_frames') if platform=='windows' else ('decoded_frames',)
    for name in bools:
        if type(value.get(name)) is not bool:raise ValueError('Missing or invalid boolean: '+name)
    if platform=='windows':
        for name in ('direct_packet_count','direct_max_pending_frames'):
            if name in value and (type(value[name]) is not int or not 0<=value[name]<=10000):raise ValueError('Invalid packet measurement: '+name)
    for name in counts:
        if type(value.get(name)) is not int or not 0<=value[name]<=10000:raise ValueError('Missing or invalid count: '+name)
    return value

def load(path:Path):
    with path.open('rb') as handle:data=handle.read(65537)
    if len(data)>65536:raise ValueError('Probe report exceeds64KiB')
    try:return validate(json.loads(data.decode('utf-8')))
    except (UnicodeError,json.JSONDecodeError) as error:raise ValueError('Invalid probe JSON') from error

def assess(windows:dict,mac:dict):
    w=validate(windows);m=validate(mac)
    if w['platform']!='windows' or m['platform']!='macos':raise ValueError('Expected Windows and Mac reports')
    r={'windows_arm_capture':w['architecture']=='arm64' and w['capture_supported'] and w['capture_frames']>0 and w['owned_pixels_verified'],
       'windows_arm_h264':w['architecture']=='arm64' and w['encoded_frames']==30 and w['software_h264_count']>0,
       'low_latency_configuration':w['low_latency_applied'] and w['zero_b_frames_applied'],
       'mac_h264_decode':m['architecture']=='arm64' and m['decoded_frames']==30 and m['dimensions_verified'] and m['pixel_variation_verified'],
       'mac_hardware_decode':m['architecture']=='arm64' and m['hardware_decoder_used'],
       'native_surface_submission':m['owned_surface_created'] and m['surface_rendering']}
    r['candidate_backend']=all(r.values())
    r['gate_a_complete']=r['candidate_backend'] and w['raw_packet_latency_measured'] and w.get('direct_packet_count',0)==30 and 0<=w.get('direct_max_pending_frames',10000)<=2 and m['copy_path_measured'] and m['onscreen_pixels_verified']
    r['continuous_video_accepted']=False
    r['performance_accepted']=False
    return r

if __name__=='__main__':
    import sys
    if len(sys.argv)!=3:raise SystemExit('Usage: report.py WINDOWS_REPORT MAC_REPORT')
    print(json.dumps(assess(load(Path(sys.argv[1])),load(Path(sys.argv[2]))),indent=2))
