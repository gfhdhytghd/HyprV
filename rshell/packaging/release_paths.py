"""Deployment references must outlive the build: no temporary/frozen workspaces."""
import pathlib
import pwd


def stable_path(value,allow_build_target=False):
    if not isinstance(value,str) or not pathlib.Path(value).is_absolute():
        raise RuntimeError('deployment path must be absolute: '+str(value))
    path=pathlib.Path(value)
    def ephemeral(p):
        return any(p==root or root in p.parents for root in map(pathlib.Path,('/tmp','/var/tmp','/run','/dev/shm'))) or any(
            (part in ('scratch','scratchpad','snapshots','snapshot','.build')or(part=='target'and not allow_build_target)) or part.startswith(('snapshot-','frozen-')) for part in p.parts)
    if ephemeral(path):raise RuntimeError('temporary/snapshot deployment path refused: '+value)
    try:resolved=path.resolve(strict=True)
    except OSError as error:raise RuntimeError('deployment path missing: '+value)from error
    if ephemeral(resolved):raise RuntimeError('temporary/snapshot deployment path refused: '+str(resolved))
    return resolved


def validate_metadata(metadata):
    expected={'schema','workspace','owner','sources_sha256'}
    if metadata.get('schema')==2:
        expected|={'deployment_profile','cpu_power'}
        if metadata.get('deployment_profile')!='laptop'or not isinstance(metadata.get('cpu_power'),bool):
            raise RuntimeError('invalid laptop deployment capabilities')
    elif metadata.get('schema')!=1:
        raise RuntimeError('invalid release metadata fields/schema')
    if set(metadata)!=expected:
        raise RuntimeError('invalid release metadata fields/schema')
    workspace=stable_path(metadata['workspace'])
    for name in ('Cargo.toml','cutover/preflight.py','packaging/install.py'):
        path=stable_path(str(workspace/name))
        if not path.is_file():raise RuntimeError('deployment file missing: '+str(path))
    if not stable_path(str(workspace.parent/'hypr')).is_dir():raise RuntimeError('Hyprland configuration directory missing')
    owner=metadata['owner']
    if not isinstance(owner,str)or not owner:raise RuntimeError('release desktop owner missing')
    try:account=pwd.getpwnam(owner)
    except KeyError as error:raise RuntimeError('release desktop owner does not exist: '+owner)from error
    stable_path(account.pw_dir)
    digest=metadata['sources_sha256']
    if not isinstance(digest,str)or len(digest)!=64 or any(c not in '0123456789abcdef'for c in digest):raise RuntimeError('invalid release source digest')
    return workspace


def validate_installed_record(record,metadata):
    validate_metadata(metadata)
    if record.get('workspace')!=metadata['workspace'] or record.get('owner')!=metadata['owner']:
        raise RuntimeError('installed deployment workspace/owner differs from release; reinstall corrected release')
    name=record.get('dir','')
    if not isinstance(name,str)or not name or pathlib.PurePath(name).name!=name or name in ('.','..'):
        raise RuntimeError('unsafe installed release directory')
    for destination in record.get('files',{}):stable_path(destination)


def deployment_options(metadata):
    validate_metadata(metadata)
    return metadata.get('deployment_profile','desktop'),metadata.get('cpu_power',False)
