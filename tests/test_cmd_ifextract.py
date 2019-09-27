"""Tests `oasis ifextract`."""

import json
import os.path as osp
from subprocess import PIPE

FIXTURE_WASM = osp.abspath(osp.join(osp.dirname(__file__), 'res', 'fixture.wasm'))


def _iface_is_sane(iface):
    return iface['name'] == 'Fixture' and 'oasis_build_version' in iface


def test_ifextract_to_cwd(oenv):
    oenv.run(f'oasis ifextract {FIXTURE_WASM}')
    with open(osp.join(oenv.home_dir, 'Fixture.json')) as f_iface:
        assert _iface_is_sane(json.load(f_iface))


def test_ifextract_to_dir(oenv):
    oenv.run('mkdir iface_dir')
    oenv.run(f'oasis ifextract file://{FIXTURE_WASM} --out iface_dir')
    with open(osp.join(oenv.home_dir, 'iface_dir', 'Fixture.json')) as f_iface:
        assert _iface_is_sane(json.load(f_iface))


def test_ifextract_to_stdout(oenv):
    output = oenv.run(f'oasis ifextract file://{FIXTURE_WASM} --out -', stdout=PIPE)
    assert _iface_is_sane(json.loads(output.stdout))
