"""Tests `oasis config` and the generation of the config files."""

import os.path as osp
import re
from subprocess import PIPE

from .common import run
from .common import default_config, telemetry_config, oenv  # pylint:disable=unused-import


def test_firstrun_dialog(oenv):
    cp = run('oasis', input='', stdout=PIPE)
    assert cp.stdout.split('\n', 1)[0] == 'Welcome to the Oasis Development Environment!'

    assert osp.isfile(oenv.config_file)
    with open(oenv.config_file) as f_cfg:
        assert f_cfg.read().find('[telemetry]\nenabled = false') != -1


def test_firstrun_skip_dialog(oenv):
    envs = {'OASIS_SKIP_GENERATE_CONFIG': '1'}
    cp = run('oasis', envs=envs, stdout=PIPE)
    assert re.match(r'oasis \d+\.\d+\.\d+', cp.stdout.split('\n', 1)[0])
    assert not osp.exists(oenv.config_file)


def test_init(oenv, default_config):
    run('oasis init test')
    with open('test/service/Cargo.toml') as f_cargo:
        assert f_cargo.read().startswith('[package]\nname = "test"')
    assert not osp.exists(oenv.metrics_file)


def test_telemetry_enabled(oenv, telemetry_config):
    cp = run('oasis config telemetry status', stdout=PIPE)
    assert cp.stdout == \
            f'Telemetry is enabled.\nUsage data is being written to `{oenv.metrics_file}`\n'

    run('oasis init test')
    assert osp.isfile(oenv.metrics_file)

    run('oasis config telemetry disable')
    with open(oenv.config_file) as f_cfg:
        assert f_cfg.read().find('[telemetry]\nenabled = false') != -1
