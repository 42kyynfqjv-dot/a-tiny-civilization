#!/usr/bin/env python3
"""Compile exact Amniote life-history fields without filling gaps."""
import argparse, csv, hashlib, json, os, struct, tempfile
from decimal import Decimal, InvalidOperation
from pathlib import Path
MAGIC=b"ATCGBF01"
def sha(b): return hashlib.sha256(b).hexdigest()
def text(f): return f.read(struct.unpack("<I",f.read(4))[0]).decode()
def names(path):
 out={}
 with open(path,"rb") as f:
  if f.read(8)!=MAGIC or struct.unpack("<H",f.read(2))[0]!=1: raise RuntimeError("unsupported GBIF catalog")
  f.read(32)
  for _ in range(struct.unpack("<Q",f.read(8))[0]):
   key=struct.unpack("<Q",f.read(8))[0]; scientific,canonical=text(f),text(f)
   for _ in range(5): text(f)
   out.setdefault(canonical,[]).append((key,scientific))
 return out
def main():
 p=argparse.ArgumentParser(); p.add_argument("--catalog",type=Path,required=True);p.add_argument("--database",type=Path,required=True);p.add_argument("--output",type=Path,required=True);a=p.parse_args()
 index=names(a.catalog); source=a.database.read_bytes(); profiles=[]
 fields=(("female_maturity_d","female-maturity","d"),("male_maturity_d","male-maturity","d"),("maximum_longevity_y","maximum-longevity","y"),("adult_body_mass_g","adult-body-mass","g"))
 with a.database.open(newline="",encoding="utf-8") as f:
  for line,row in enumerate(csv.DictReader(f),2):
   name=f"{row['genus'].strip()} {row['species'].strip()}"; candidates=index.get(name,[])
   if len(candidates)!=1: continue
   key,scientific=candidates[0]
   for column,trait,unit in fields:
    raw=row[column].strip()
    try:
     value=Decimal(raw)
     if raw=="-999" or value<=0: continue
     scaled=value*1000 if unit=="y" else value
     if scaled!=scaled.to_integral_value(): continue
    except (InvalidOperation,ValueError): continue
    record={"line":line,"scientific_name":name,"column":column,"value":raw}
    profiles.append({"species":{"catalog":"gbif","identifier":str(key),"scientific_name":scientific,"source_url":f"https://www.gbif.org/species/{key}"},"trait_id":trait,"value":{"value":int(scaled),"decimal_places":0,"unit":"milli-y" if unit=="y" else unit},"source":"amniote-life-history-2015-08","source_field":column,"source_record_id":f"amniote-database-line-{line}","source_record_digest":sha(json.dumps(record,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()),"evidence_basis":"source_compiled_species_aggregate"})
 profiles.sort(key=lambda v:(v["species"]["catalog"],v["species"]["identifier"],v["trait_id"],v["source_record_id"]))
 if not profiles: raise RuntimeError("no exact positive life-history values")
 data=json.dumps({"profile_set_schema_version":1,"source_artifact_digest":sha(source),"profiles":profiles},separators=(",",":"),ensure_ascii=False).encode()
 if a.output.exists(): raise RuntimeError("refusing to replace output")
 a.output.parent.mkdir(parents=True,exist_ok=True);fd,tmp=tempfile.mkstemp(dir=a.output.parent,prefix=".life-history-")
 try:
  with os.fdopen(fd,"wb") as f:f.write(data)
  os.replace(tmp,a.output)
 finally:
  if os.path.exists(tmp):os.unlink(tmp)
 print(json.dumps({"content_hash":sha(data),"profile_count":len(profiles),"output_path":str(a.output)}))
if __name__=="__main__":main()
