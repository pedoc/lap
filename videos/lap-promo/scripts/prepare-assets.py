"""Freeze the repository's documented public demo assets; never read personal libraries."""
from pathlib import Path
import json
import shutil
import subprocess
import wave
import numpy as np
from PIL import Image, ImageDraw

PROJECT = Path(__file__).resolve().parents[1]
REPO = PROJECT.parents[1]
SKILL = Path.home() / '.agents/skills/video-shotcraft'
PUBLIC = PROJECT / 'public'
for name in ['photos', 'screens', 'audio', 'crops']:
    (PUBLIC / name).mkdir(parents=True, exist_ok=True)
(PROJECT / 'out/qa').mkdir(parents=True, exist_ok=True)

for name in ['home', 'combined-filters', 'smart-albums', 'media-chat', 'objects', 'ocr', 'map-amap-3d', 'map-amap-satellite']:
    shutil.copy2(REPO / f'docs/public/screenshots/current/{name}.webp', PUBLIC / f'screens/{name}.webp')
shutil.copy2(REPO / 'docs/public/icon.png', PUBLIC / 'lap.png')
demo = REPO / '.codex/readme-demo-media/photos'
for short, fragment in [('waterfall','20_iceland-waterfall'), ('mountain','01_mountain-wilderness'), ('peak','04_matterhorn-sunset')]:
    matches = [p for p in demo.rglob('*.jpg') if fragment in p.name and 'Duplicates' not in p.parts]
    assert len(matches) == 1, (short, matches)
    # Strip demonstration EXIF; use only the pixels in the authored video.
    with Image.open(matches[0]) as im:
        im.convert('RGB').save(PUBLIC / f'photos/{short}.jpg', quality=95, subsampling=0)

# Crops are exact crops of native screenshots, without redrawing the product UI.
crops = {
    'ocr': ('ocr', (329, 147, 1430, 1000)),
    'objects': ('objects', (328, 168, 1438, 1005)),
    'filters': ('combined-filters', (280, 110, 1405, 955)),
    'chat': ('media-chat', (287, 155, 1387, 875)),
    'filter-dialog': ('combined-filters', (410, 289, 1030, 970)),
    'ocr-focus': ('ocr', (328, 204, 1440, 970)),
}
layout = {}
for name, (source, box) in crops.items():
    with Image.open(PUBLIC / f'screens/{source}.webp') as im:
        im.crop(box).save(PUBLIC / f'crops/{name}.webp', lossless=True)
    layout[name] = {'source': f'screens/{source}.webp', 'x':box[0], 'y':box[1], 'w':box[2]-box[0], 'h':box[3]-box[1]}
(PUBLIC / 'layout.json').write_text(json.dumps(layout, indent=2), encoding='utf-8')
with Image.open(PUBLIC/'screens/smart-albums.webp') as source:
    plate=source.convert('RGB').copy()
    draw=ImageDraw.Draw(plate)
    for i,x in enumerate([337,628,919]):
        box=(x,95,x+275,405)
        source.crop(box).save(PUBLIC/f'crops/album-{i}.webp',lossless=True)
        draw.rectangle(box,fill=(21,25,30))
    plate.save(PUBLIC/'crops/albums-backplate.webp',lossless=True)

for folder, name in [('bgm','cat-walk'), ('sfx/transition','sweep-fast-small'), ('sfx/transition','air-zoom-vacuum'), ('sfx/impact','bass-hit-short'), ('sfx/impact','impact-deep-whoosh'), ('sfx/camera','camera-shutter-hard'), ('sfx/light','shimmer-sparkle-sweep')]:
    shutil.copy2(SKILL / f'assets/audio/{folder}/{name}.mp3', PUBLIC / f'audio/{name}.mp3')
    if folder.startswith('sfx/'):
        subprocess.run(['ffmpeg','-v','error','-y','-i',str(PUBLIC/f'audio/{name}.mp3'),'-ar','48000','-ac','2','-c:a','pcm_s16le',str(PUBLIC/f'audio/{name}.wav')],check=True)

# Inspect the recorded music rather than relying on the approximate BPM in its label.
wav = PROJECT / 'out/qa/music-analysis.wav'
subprocess.run(['ffmpeg','-v','error','-y','-i',str(PUBLIC/'audio/cat-walk.mp3'),'-t','50','-ac','1','-ar','22050',str(wav)], check=True)
with wave.open(str(wav)) as w:
    sr=w.getframerate(); x=np.frombuffer(w.readframes(w.getnframes()), dtype='<i2').astype(float)/32768
hop=220; size=1024
frames=np.lib.stride_tricks.sliding_window_view(x,size)[::hop]
spec=np.abs(np.fft.rfft(frames*np.hanning(size),axis=1))
freq=np.fft.rfftfreq(size,1/sr)
bass=np.sqrt((spec[:,(freq>35)&(freq<180)]**2).mean(axis=1))
onset=np.maximum(0, np.diff(bass,prepend=bass[0]))
onset=np.convolve(onset, np.array([.2,.6,.2]), mode='same')
onset/=max(np.percentile(onset,95),1e-9)
times=np.arange(len(onset))*hop/sr
best=(-1,None,None)
for bpm in np.arange(125,133,.025):
    period=60/bpm
    for phase in np.arange(0,period,.006):
        beats=np.arange(phase,45,period)
        idx=np.rint(beats*sr/hop).astype(int)
        score=np.minimum(onset[idx],4).mean()
        if score>best[0]: best=(float(score),float(bpm),float(phase))
score,bpm,phase=best
beats=np.arange(phase,45,60/bpm)
# Choose a strong bar near the opening; keep the soundtrack's original speed.
start_candidates=beats[(beats>=3)&(beats<12)]
energy=[np.mean(x[int(t*sr):int((t+2)*sr)]**2) for t in start_candidates]
start=float(start_candidates[int(np.argmax(energy))])
duration=72*60/bpm
subprocess.run(['ffmpeg','-v','error','-y','-ss',str(start),'-i',str(PUBLIC/'audio/cat-walk.mp3'),'-t',str(duration),'-af','afade=t=in:d=0.15,afade=t=out:st='+str(duration-1.25)+':d=1.25','-ar','48000','-c:a','pcm_s16le',str(PUBLIC/'audio/music.wav')],check=True)
report={'track':'Cat Walk / Arulo','bpm':round(bpm,4),'sourcePhaseSeconds':phase,'sourceTrimSeconds':start,'durationSeconds':duration,'totalFrames':round(duration*30),'gridScore':score,'cutBeatNumbers':[0,8,16,28,36,44,52,60,72]}
(PROJECT/'out/qa/music-grid.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
print(json.dumps(report))
