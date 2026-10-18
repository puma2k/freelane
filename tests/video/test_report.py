import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC=importlib.util.spec_from_file_location('video_report',Path(__file__).resolve().parents[2]/'scripts/video/report.py')
report=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(report)

def fixtures():
    return ({'schema':1,'platform':'windows','architecture':'arm64','capture_supported':True,'capture_frames':1,'owned_pixels_verified':True,'software_h264_count':1,'hardware_h264_count':0,'encoded_frames':30,'low_latency_applied':True,'zero_b_frames_applied':True,'raw_packet_latency_measured':False},
            {'schema':1,'platform':'macos','architecture':'arm64','decoded_frames':30,'dimensions_verified':True,'pixel_variation_verified':True,'hardware_decoder_used':True,'owned_surface_created':True,'surface_rendering':True,'onscreen_pixels_verified':False,'copy_path_measured':False})
class ReportTests(unittest.TestCase):
    def test_arm_native_candidate_is_not_end_to_end_acceptance(self):
        w,m=fixtures();r=report.assess(w,m)
        self.assertTrue(r['candidate_backend']);self.assertFalse(r['gate_a_complete']);self.assertFalse(r['continuous_video_accepted']);self.assertFalse(r['performance_accepted'])
    def test_x64_execution_cannot_prove_arm_encoder_or_capture(self):
        w,m=fixtures();w['architecture']='x86_64';r=report.assess(w,m)
        self.assertFalse(r['windows_arm_capture']);self.assertFalse(r['windows_arm_h264']);self.assertFalse(r['candidate_backend'])
    def test_codec_enumeration_without_frames_is_not_functional_proof(self):
        w,m=fixtures();w['encoded_frames']=0;m['decoded_frames']=0
        self.assertFalse(report.assess(w,m)['candidate_backend'])
    def test_requested_settings_and_surface_creation_do_not_imply_readback(self):
        w,m=fixtures();w['low_latency_applied']=False
        self.assertFalse(report.assess(w,m)['candidate_backend'])
        w,m=fixtures();m['hardware_decoder_used']=False
        self.assertFalse(report.assess(w,m)['mac_hardware_decode'])
    def test_raw_packet_flag_alone_cannot_accept_the_streaming_budget(self):
        w,m=fixtures();w['raw_packet_latency_measured']=True;m['copy_path_measured']=True;m['onscreen_pixels_verified']=True
        self.assertFalse(report.assess(w,m)['gate_a_complete'])
        w['direct_packet_count']=30;w['direct_max_pending_frames']=2
        self.assertFalse(report.assess(w,m)['gate_a_complete'])
        w['direct_low_latency_applied']=True;w['direct_zero_b_frames_applied']=True
        self.assertTrue(report.assess(w,m)['gate_a_complete'])
        w['direct_max_pending_frames']=3
        self.assertFalse(report.assess(w,m)['gate_a_complete'])
    def test_wrong_types_and_counts_fail_closed(self):
        for key,value in [('owned_pixels_verified',1),('encoded_frames',True),('schema',2),('encoded_frames',-1),('architecture','unknown')]:
            w,m=fixtures();w[key]=value
            with self.assertRaises(ValueError):report.assess(w,m)
    def test_report_file_is_bounded_and_requires_correct_platform(self):
        w,m=fixtures()
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'report.json';p.write_text(json.dumps(w));self.assertEqual(report.load(p)['platform'],'windows')
            p.write_text(' '*65537)
            with self.assertRaises(ValueError):report.load(p)
        w['platform']='linux'
        with self.assertRaises(ValueError):report.assess(w,m)
if __name__=='__main__':unittest.main()
