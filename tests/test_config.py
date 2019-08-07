"""Tests `oasis config` and the generation of the config files."""

import os.path as osp
import re
from subprocess import PIPE


def test_firstrun_dialog(oenv):
    oenv.no_config()
    cp = oenv.run('oasis', input='', stdout=PIPE)
    assert cp.stdout.split('\n', 1)[0] == 'Welcome to the Oasis Development Environment!'

    assert osp.isfile(oenv.config_file)
    assert not oenv.load_config()['telemetry']['enabled']
    with open(oenv.config_file) as f_cfg:
        assert f_cfg.read().find('[telemetry]\nenabled = false') != -1


def test_firstrun_skip_dialog(oenv):
    oenv.no_config()
    env = {'OASIS_SKIP_GENERATE_CONFIG': '1'}
    cp = oenv.run('oasis', env=env, stdout=PIPE)
    assert re.match(r'oasis \d+\.\d+\.\d+', cp.stdout.split('\n', 1)[0])  # oasis x.y.z
    assert not osp.exists(oenv.config_file)


def test_init(oenv):
    oenv.run('oasis init test')
    with open(osp.join(oenv.home_dir, 'test/service/Cargo.toml')) as f_cargo:
        assert f_cargo.read().startswith('[package]\nname = "test"')
    assert not osp.exists(oenv.metrics_file)


def test_telemetry_enabled(oenv):
    oenv.telemetry_config()
    cp = oenv.run('oasis config telemetry.enabled', stdout=PIPE)
    assert cp.stdout == 'true\n'

    oenv.run('oasis init test')
    assert osp.isfile(oenv.metrics_file)

    oenv.run('oasis config telemetry.enabled false')
    assert not oenv.load_config()['telemetry']['enabled']


def test_edit_invalid_key(oenv):
    cp = oenv.run('oasis config profile.default.num_tokens 9001', stderr=PIPE, check=False)
    assert 'unknown configuration option: `num_tokens`. Valid options are' in cp.stderr


def test_get_invalid_key(oenv):
    cp = oenv.run('oasis config config.oasis', stdout=PIPE)
    assert not cp.stdout


def test_edit_secret(oenv):
    """Tests that mnemonic/private_key can be set and are mutually exclusive."""
    mnemonic = 'patient oppose cottion ...'
    oenv.run(f'oasis config profile.default.mnemonic "{mnemonic}"')
    assert oenv.load_config()['profile']['default']['mnemonic'] == mnemonic

    skey = 'p7PFqoZsBAUBxqTBv93DthnxrVkNt7sg'
    oenv.run(f'oasis config profile.default.private_key "{skey}"')
    updated = oenv.load_config()['profile']['default']
    assert updated['private_key'] == skey
    assert 'mnemonic' not in updated

    oenv.run(f'oasis config profile.default.mnemonic "{mnemonic}"')
    assert 'private_key' not in oenv.load_config()['profile']['default']
    cp = oenv.run('oasis config profile.default.mnemonic', stdout=PIPE)
    assert cp.stdout == f'"{mnemonic}"\n'


def test_edit_endpoint(oenv):
    endpoint = 'wss://gateway.oasiscloud.io'
    oenv.run(f'oasis config profile.local.endpoint "{endpoint}"')
    assert oenv.load_config()['profile']['local']['endpoint'] == endpoint
    cp = oenv.run('oasis config profile.local.endpoint', stdout=PIPE)
    assert cp.stdout == f'"{endpoint}"\n'
