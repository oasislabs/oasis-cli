"""Tests `oasis config` and the generation of the config files."""

import os.path as osp
import re
from subprocess import PIPE

from .conftest import SAMPLE_KEY, SAMPLE_MNEMONIC, SAMPLE_TOKEN


def test_firstrun_dialog(oenv):
    oenv.no_config()
    cp = oenv.run('oasis', input='', stdout=PIPE)
    assert cp.stdout.split('\n', 1)[0] == 'Welcome to the Oasis Development Environment!'

    cp = oenv.run('oasis config telemetry.enabled', stdout=PIPE)
    assert cp.stdout.rstrip() == 'false'


def test_firstrun_skip_dialog(oenv):
    oenv.no_config()
    env = {'OASIS_SKIP_GENERATE_CONFIG': '1'}
    cp = oenv.run('oasis', env=env, stdout=PIPE)
    assert re.match(r'oasis \d+\.\d+\.\d+', cp.stdout.split('\n', 1)[0])  # oasis x.y.z
    assert not osp.exists(oenv.config_file)


def test_init(oenv):
    oenv.run('oasis init test')
    with open(osp.join(oenv.home_dir, 'test/services/Cargo.toml')) as f_cargo:
        assert f_cargo.read().startswith('[package]\nname = "test"')
    assert not osp.exists(oenv.metrics_file)


def test_telemetry_enabled(oenv):
    oenv.telemetry_config()
    cp = oenv.run('oasis config telemetry.enabled', stdout=PIPE)
    assert cp.stdout == 'true\n'

    oenv.run('oasis init test')
    assert osp.isfile(oenv.metrics_file)


def test_edit_invalid_key(oenv):
    cp = oenv.run('oasis config profile.default.num_tokens 9001', stderr=PIPE, check=False)
    assert 'unknown profile configuration key `num_tokens`' in cp.stderr


def test_get_invalid_key(oenv):
    cp = oenv.run('oasis config config.oasis', stdout=PIPE)
    assert not cp.stdout


def test_edit_credential(oenv):
    def _roundtrip(credential):
        oenv.run(f'oasis config profile.default.credential "{credential}"')
        cp = oenv.run('oasis config profile.default.credential', stdout=PIPE)
        assert cp.stdout.rstrip() == credential

    _roundtrip(SAMPLE_KEY)
    _roundtrip(SAMPLE_MNEMONIC)
    _roundtrip(SAMPLE_TOKEN)


def test_edit_credential_invalid(oenv):
    cp = oenv.run(f'oasis config profile.local.credential "abcdef"', check=False, stderr=PIPE)
    assert 'invalid' in cp.stderr


def test_edit_from_stdin(oenv):
    oenv.run(f'oasis config profile.default.credential -', input=f'{SAMPLE_MNEMONIC}\n')
    cp = oenv.run('oasis config profile.default.credential', stdout=PIPE)
    assert cp.stdout.rstrip() == SAMPLE_MNEMONIC


def test_edit_gateway(oenv):
    gateway = 'ws://localhost:8546'
    cp = oenv.run('oasis config profile.local.gateway', stdout=PIPE)
    assert cp.stdout.rstrip() == gateway


def test_edit_gateway_invalid(oenv):
    cp = oenv.run(f'oasis config profile.default.gateway "not://a-url!"', check=False, stderr=PIPE)
    assert 'invalid' in cp.stderr
