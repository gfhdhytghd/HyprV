#!/usr/bin/env python3
"""Snapshot build inputs inside probe ownership; never update the shared lockfile."""
import hashlib,json,pathlib,shutil,subprocess,sys,time
crate=pathlib.Path(__file__).resolve().parents[1]
root=crate.parents[1]
check=crate/'.check'; (check/'crates').mkdir(parents=True,exist_ok=True)
names=['hyprv-core','hyprv-privhelper','hyprv-svc-net','hyprv-svc-media','hyprv-svc-power','hyprv-svc-sys','hyprv-svc-desktop','hyprv-svc-probe']
hashes={}
for name in names:
    source=root/'crates'/name; destination=check/'crates'/name
    destination.mkdir(exist_ok=True)
    for part in ['src','vendor','csrc','native','assets']:
        if (destination/part).exists():shutil.rmtree(destination/part)
    (destination/'build.rs').unlink(missing_ok=True)
    for file in [source/'Cargo.toml',*([source/'build.rs'] if (source/'build.rs').exists() else []),*[file for part in ['src','vendor','csrc','native','assets'] for file in sorted((source/part).rglob('*'))]]:
        if not file.is_file():continue
        relative=file.relative_to(source)
        target=destination/relative;target.parent.mkdir(parents=True,exist_ok=True)
        data=file.read_bytes();target.write_bytes(data)
        hashes[f'{name}/{relative}']=hashlib.sha256(data).hexdigest()
(check/'Cargo.toml').write_text('[workspace]\nresolver="2"\nmembers=["crates/*"]\n[workspace.package]\nversion="0.1.0"\nedition="2021"\n')
(root/'docs/verify').mkdir(parents=True,exist_ok=True)
snapshot=json.dumps({'epoch':time.time(),'command':sys.argv[1:] or ['build'],'files':hashes},indent=2)+'\n'
(root/'docs/verify/build-snapshot.json').write_text(snapshot)
(root/'docs/verify/raw').mkdir(parents=True,exist_ok=True)
(root/'docs/verify/raw'/f'build-{time.time_ns()}.json').write_text(snapshot)
command=sys.argv[1:] or ['build']
result=subprocess.run(['cargo',command[0],'--manifest-path',str(check/'Cargo.toml'),'-p','hyprv-svc-probe',*command[1:]])
sys.exit(result.returncode)
