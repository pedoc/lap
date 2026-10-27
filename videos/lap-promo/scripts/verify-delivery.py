"""Verify the encoded deliverables, including SFX timing measured from AAC output."""
from pathlib import Path
import hashlib
import json
import subprocess
import numpy as np

P=Path(__file__).resolve().parents[1]
def run(args):
    return subprocess.run(args,capture_output=True,check=True).stdout
def pcm(path):
    return np.frombuffer(run(['ffmpeg','-v','error','-i',str(path),'-vn','-ac','1','-ar','22050','-f','f32le','-']),dtype='<f4').astype(float)
def video_hash(path):
    return run(['ffmpeg','-v','error','-i',str(path),'-map','0:v:0','-c','copy','-f','hash','-hash','sha256','-']).decode().strip()
def decoded_frame_hashes(path):
    text=run(['ffmpeg','-v','error','-i',str(path),'-map','0:v:0','-f','framemd5','-']).decode()
    return [line.rsplit(',',1)[-1].strip() for line in text.splitlines() if line and not line.startswith('#')]

timeline=json.loads((P/'out/qa/timeline.json').read_text(encoding='utf8'))
total=timeline['total']
report={'outputs':{},'audio_probes':[]}
paths={name:P/f'out/{name}.mp4' for name in ['lap-promo-4k','lap-promo-4k-nobgm','lap-promo-1080p']}
for name,path in paths.items():
    meta=json.loads(run(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(path)]))
    v=next(s for s in meta['streams'] if s['codec_type']=='video')
    a=next(s for s in meta['streams'] if s['codec_type']=='audio')
    assert int(v['nb_frames'])==total and v['r_frame_rate']=='30/1'
    assert (v['width'],v['height'])==((1920,1080) if name.endswith('1080p') else (3840,2160))
    assert v['pix_fmt']=='yuv420p' and v['color_space']=='bt709'
    assert a['codec_name']=='aac' and a['sample_rate']=='48000' and a['channels']==2
    assert abs(float(meta['format']['duration'])-total/30)<1/30
    report['outputs'][name]={'width':v['width'],'height':v['height'],'frames':total,'fps':30,'duration':meta['format']['duration'],'size':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}

report['same_4k_video_bitstream']=video_hash(paths['lap-promo-4k'])==video_hash(paths['lap-promo-4k-nobgm'])
if report['same_4k_video_bitstream']:
    report['same_decoded_4k_frames']=True
else:
    # Encoder headers may differ; compare every actual decoded frame.
    main_frames=decoded_frame_hashes(paths['lap-promo-4k'])
    silent_frames=decoded_frame_hashes(paths['lap-promo-4k-nobgm'])
    assert len(main_frames)==len(silent_frames)==total
    report['same_decoded_4k_frames']=main_frames==silent_frames
assert report['same_decoded_4k_frames'], 'Two exports differ in decoded pixels.'

sr=22050
audio=pcm(paths['lap-promo-4k-nobgm'])
for cue_index in [1,4,5]:
    cue=timeline['sfx'][cue_index]
    source=pcm(P/'public'/cue['src'])[:min(int(.5*sr),int(cue['duration']/30*sr))]
    expected=cue['from']/30
    start=max(0,int((expected-.18)*sr))
    segment=audio[start:int((expected+.18)*sr)+len(source)]
    n=1<<(len(segment)+len(source)-1).bit_length()
    corr=np.fft.irfft(np.fft.rfft(segment,n)*np.fft.rfft(source[::-1],n),n)
    valid=corr[len(source)-1:len(segment)]
    lag=int(np.argmax(valid))
    actual=(start+lag)/sr
    energy=np.convolve(source**2,np.ones(220)/220,mode='same')
    source_peak=float(np.argmax(energy))/sr
    delta_frames=(actual-expected)*30
    assert abs(delta_frames)<.5, (cue['label'],delta_frames)
    report['audio_probes'].append({'cue':cue['label'],'source_start_frame':cue['from'],'output_offset_frames':delta_frames,'source_peak_lag_seconds':source_peak})

main=pcm(paths['lap-promo-4k'])
report['mono_analysis_peak_dbfs']=float(20*np.log10(max(np.max(np.abs(main)),1e-12)))
stereo=np.frombuffer(run(['ffmpeg','-v','error','-i',str(paths['lap-promo-4k']),'-vn','-ac','2','-ar','48000','-f','f32le','-']),dtype='<f4').astype(float)
report['decoded_peak_dbfs']=float(20*np.log10(max(np.max(np.abs(stereo)),1e-12)))
report['decoded_rms_dbfs']=float(20*np.log10(max(np.sqrt(np.mean(stereo**2)),1e-12)))
assert report['decoded_peak_dbfs']<-.1, 'Encoded audio clips.'

# Find the nearest real bass transient at each scene cut in the final mixed audio.
hop=220; size=1024
frames=np.lib.stride_tricks.sliding_window_view(main,size)[::hop]
spec=np.abs(np.fft.rfft(frames*np.hanning(size),axis=1))
freq=np.fft.rfftfreq(size,1/sr)
bass=np.sqrt(np.mean(spec[:,(freq>35)&(freq<180)]**2,axis=1))
onset=np.maximum(0,np.diff(bass,prepend=bass[0]))
report['cut_transients']=[]
for shot in timeline['shots'][1:]:
    expected=shot['from']/30
    i=round(expected*sr/hop)
    lo=max(0,i-7); hi=min(len(onset),i+8)
    found=(lo+int(np.argmax(onset[lo:hi])))*hop/sr
    delta=(found-expected)*30
    assert abs(delta)<=3, (shot['label'],delta)
    report['cut_transients'].append({'scene':shot['id'],'frame':shot['from'],'nearest_onset_error_frames':delta})

(P/'out/qa/delivery-verification.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf8')
print(json.dumps(report,ensure_ascii=False,indent=2))
