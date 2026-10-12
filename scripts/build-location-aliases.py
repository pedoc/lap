"""Build Chinese display aliases for Lap's existing offline records (GeoNames CC BY 4.0).
Download inputs into an ignored cache; never replace cities.csv or stored location keys.
Usage: python scripts/build-location-aliases.py --cache src-tauri/target/geonames
"""
import argparse
import csv
import hashlib
import io
import json
import math
import unicodedata
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "https://download.geonames.org/export/dump/"

def normalize(value):
    return "".join(c for c in unicodedata.normalize("NFKD", value).casefold() if not unicodedata.combining(c))

def lines(path, name):
    with zipfile.ZipFile(path) as archive, archive.open(name) as raw:
        yield from csv.reader(io.TextIOWrapper(raw, encoding="utf-8"), delimiter="\t")

def generate(cache):
    paths = {"records": ("CN.zip", "CN.zip"), "alternates": ("alternatenames/CN.zip", "CN-alternates.zip"),
             "regions": ("admin1CodesASCII.txt", "admin1CodesASCII.txt")}
    for remote, local in paths.values():
        path = cache / local
        if not path.exists():
            print("Downloading", remote, flush=True)
            with urllib.request.urlopen(BASE + remote, timeout=60) as response, path.open("wb") as out:
                while data := response.read(1024 * 1024): out.write(data)
    regions = {}
    with (cache / "admin1CodesASCII.txt").open(encoding="utf-8") as stream:
        for row in csv.reader(stream, delimiter="\t"):
            if row[0].startswith("CN."): regions[row[0][3:]] = row
    candidates = {}
    for row in lines(cache / "CN.zip", "CN.txt"):
        if len(row) < 19 or row[6] != "P": continue
        for name in set([row[1], row[2]]): candidates.setdefault(normalize(name), []).append(row)
    matched, old_regions, missing = {}, {}, 0
    with (ROOT / "src-tauri/data/cities.csv").open(encoding="utf-8") as stream:
        for old in csv.DictReader(stream):
            if old["cc"] != "CN": continue
            lat, lon = float(old["lat"]), float(old["lon"])
            choices = candidates.get(normalize(old["name"]), [])
            ranked = sorted(((math.hypot(float(r[4])-lat, (float(r[5])-lon)*math.cos(math.radians(lat))), r) for r in choices), key=lambda item:item[0])
            if not ranked or ranked[0][0] > 0.05 or (len(ranked)>1 and abs(ranked[0][0]-ranked[1][0]) < 1e-7):
                missing += 1; continue
            row = ranked[0][1]
            old_regions.setdefault(old["admin1"], set()).add(row[10])
            key = "|".join([old["cc"], old["admin1"], old["name"]])
            matched.setdefault(key, set()).add(row[0])
    wanted = {id for ids in matched.values() for id in ids} | {row[3] for row in regions.values()}
    aliases = {}
    # Only explicitly Chinese-language, current names. Do not guess characters from pinyin.
    for row in lines(cache / "CN-alternates.zip", "CN.txt"):
        if len(row)<4 or row[1] not in wanted or row[2] not in ["zh", "zh-CN", "zh-Hans"]: continue
        row += [""] * (8-len(row))
        if row[7] == "1" or not any("\u3400" <= c <= "\u9fff" for c in row[3]): continue
        score = ({"zh-Hans":0, "zh-CN":1, "zh":2}[row[2]], row[4]!="1", len(row[3]), row[3])
        if row[1] not in aliases or score < aliases[row[1]][0]: aliases[row[1]] = (score, row[3])
    region_names, place_names = {}, {}
    for row in regions.values():
        if row[3] in aliases:
            for raw in set(row[1:3]): region_names["CN|"+raw] = aliases[row[3]][1]
    for raw, codes in old_regions.items():
        labels = {aliases[regions[code][3]][1] for code in codes if code in regions and regions[code][3] in aliases}
        if len(labels) == 1: region_names["CN|"+raw] = labels.pop()
    for key, ids in matched.items():
        names = {aliases[id][1] for id in ids if id in aliases}
        # Different homonymous places must not be mislabeled by a lossy old DB key.
        if len(names) == 1 and len(ids) == sum(id in aliases for id in ids): place_names[key] = names.pop()
    result = {"source":"GeoNames", "license":"CC BY 4.0", "scope":"CN",
              "inputSha256":{local:hashlib.sha256((cache/local).read_bytes()).hexdigest() for _,local in paths.values()},
              "regions":dict(sorted(region_names.items())), "places":dict(sorted(place_names.items()))}
    output = ROOT / "src-vite/src/assets/location-aliases-zh.json"
    output.write_bytes((json.dumps(result,ensure_ascii=False,separators=(",",":"))+"\n").encode("utf-8"))
    print(f"Chinese region aliases: {len(region_names)}; place aliases: {len(place_names)}; unmatched records: {missing}; bytes: {output.stat().st_size}")
    for name in ["Chaowai","Chengbei","Datun","Jingshan","Jiugong","Liqiao","Qinghe","Taihu","Xiangshan","Xibeiwang"]:
        print(name,place_names.get("CN|Beijing|"+name))

if __name__ == "__main__":
    parser = argparse.ArgumentParser(); parser.add_argument("--cache",type=Path,default=ROOT/"src-tauri/target/geonames")
    args = parser.parse_args(); args.cache.mkdir(parents=True,exist_ok=True); generate(args.cache)
